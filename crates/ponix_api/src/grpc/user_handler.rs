use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info, instrument};

use crate::domain::{
    GetUserRequest as DomainGetUserRequest, RegisterUserRequest as DomainRegisterUserRequest,
    UserService,
};
use common::auth::{AuthTokenProvider, LoginUserInput, LogoutInput, RefreshTokenInput};
use common::grpc::{
    domain_error_to_status, extract_refresh_token_from_cookies, extract_user_context,
    RefreshTokenCookie,
};
use common::proto::to_proto_user;
use ponix_proto_prost::user::v1::{
    GetUserRequest, GetUserResponse, LoginRequest, LoginResponse, LogoutRequest, LogoutResponse,
    RefreshRequest, RefreshResponse, RegisterUserRequest, RegisterUserResponse,
};
use ponix_proto_tonic::user::v1::tonic::user_service_server::UserService as UserServiceTrait;

/// gRPC handler for UserService
pub struct UserServiceHandler {
    domain_service: Arc<UserService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
    refresh_token_expiration_days: u64,
    secure_cookies: bool,
}

impl UserServiceHandler {
    pub fn new(
        domain_service: Arc<UserService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
        refresh_token_expiration_days: u64,
        secure_cookies: bool,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
            refresh_token_expiration_days,
            secure_cookies,
        }
    }
}

#[tonic::async_trait]
impl UserServiceTrait for UserServiceHandler {
    #[instrument(
        name = "RegisterUser",
        skip(self, request),
        fields(email = %request.get_ref().email)
    )]
    async fn register_user(
        &self,
        request: Request<RegisterUserRequest>,
    ) -> Result<Response<RegisterUserResponse>, Status> {
        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainRegisterUserRequest {
            email: req.email,
            password: req.password,
            name: req.name,
        };

        // Call domain service
        let user = self
            .domain_service
            .register_user(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(user_id = %user.id, "User registered successfully");

        // Convert domain -> proto
        let proto_user = to_proto_user(user);

        Ok(Response::new(RegisterUserResponse {
            user: Some(proto_user),
        }))
    }

    #[instrument(
        name = "GetUser",
        skip(self, request),
        fields(user_id = %request.get_ref().user_id)
    )]
    async fn get_user(
        &self,
        request: Request<GetUserRequest>,
    ) -> Result<Response<GetUserResponse>, Status> {
        // Extract authenticated user from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request with requesting user for authorization
        let service_request = DomainGetUserRequest {
            requesting_user_id: user_context.user_id,
            user_id: req.user_id,
        };

        // Call domain service (authorization check happens in domain layer)
        let user = self
            .domain_service
            .get_user(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain -> proto
        let proto_user = to_proto_user(user);

        Ok(Response::new(GetUserResponse {
            user: Some(proto_user),
        }))
    }

    #[instrument(
        name = "Login",
        skip(self, request),
        fields(email = %request.get_ref().email)
    )]
    async fn login(
        &self,
        request: Request<LoginRequest>,
    ) -> Result<Response<LoginResponse>, Status> {
        let req = request.into_inner();

        // Construct domain request directly
        let login_input = LoginUserInput {
            email: req.email,
            password: req.password,
        };

        // Call domain service
        let output = self
            .domain_service
            .login_user(login_input)
            .await
            .map_err(domain_error_to_status)?;

        debug!("User logged in successfully");

        // Build response with access token in body
        let mut response = Response::new(LoginResponse {
            token: output.access_token,
        });

        // Set refresh token as HTTP-only cookie
        let cookie = RefreshTokenCookie::new(
            output.refresh_token,
            self.refresh_token_expiration_days,
            self.secure_cookies,
        );
        cookie.add_to_metadata(response.metadata_mut());

        Ok(response)
    }

    #[instrument(name = "Refresh", skip(self, request))]
    async fn refresh(
        &self,
        request: Request<RefreshRequest>,
    ) -> Result<Response<RefreshResponse>, Status> {
        // Log what the server sees for debugging
        let cookie_header = request
            .metadata()
            .get("cookie")
            .and_then(|v| v.to_str().ok());
        let extracted_token = extract_refresh_token_from_cookies(request.metadata());
        info!(
            cookie_header = ?cookie_header,
            extracted_token = ?extracted_token,
            "Refresh endpoint - received cookie header"
        );

        let refresh_token =
            extracted_token.ok_or_else(|| Status::unauthenticated("Refresh token not found"))?;

        let output = self
            .domain_service
            .refresh_token(RefreshTokenInput { refresh_token })
            .await
            .map_err(domain_error_to_status)?;

        debug!("Token refreshed successfully");

        let mut response = Response::new(RefreshResponse {
            access_token: output.access_token,
        });

        // Set new refresh token cookie (rotation)
        let cookie = RefreshTokenCookie::new(
            output.refresh_token,
            self.refresh_token_expiration_days,
            self.secure_cookies,
        );
        cookie.add_to_metadata(response.metadata_mut());

        Ok(response)
    }

    #[instrument(name = "Logout", skip(self, request))]
    async fn logout(
        &self,
        request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        // Try to extract and invalidate the refresh token if present
        if let Some(refresh_token) = extract_refresh_token_from_cookies(request.metadata()) {
            self.domain_service
                .logout(LogoutInput { refresh_token })
                .await
                .map_err(domain_error_to_status)?;
        }

        debug!("User logged out successfully");

        let mut response = Response::new(LogoutResponse {});

        // Clear the refresh token cookie
        let cookie = RefreshTokenCookie::clear(self.secure_cookies);
        cookie.add_to_metadata(response.metadata_mut());

        Ok(response)
    }
}
