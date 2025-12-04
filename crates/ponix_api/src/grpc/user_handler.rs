use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::UserService;
use ponix_proto_prost::user::v1::{
    GetUserRequest, GetUserResponse, LoginRequest, LoginResponse, RegisterUserRequest,
    RegisterUserResponse,
};
use ponix_proto_tonic::user::v1::tonic::user_service_server::UserService as UserServiceTrait;

use common::grpc::domain_error_to_status;
use common::proto::{to_get_user_input, to_login_user_input, to_proto_user, to_register_user_input};

/// gRPC handler for UserService
pub struct UserServiceHandler {
    domain_service: Arc<UserService>,
}

impl UserServiceHandler {
    pub fn new(domain_service: Arc<UserService>) -> Self {
        Self { domain_service }
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

        // Convert proto -> domain
        let input = to_register_user_input(req);

        // Call domain service
        let user = self
            .domain_service
            .register_user(input)
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
        let req = request.into_inner();

        // Convert proto -> domain
        let input = to_get_user_input(req);

        // Call domain service
        let user = self
            .domain_service
            .get_user(input)
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

        // Convert proto -> domain
        let input = to_login_user_input(req);

        // Call domain service
        let output = self
            .domain_service
            .login_user(input)
            .await
            .map_err(domain_error_to_status)?;

        debug!("User logged in successfully");

        Ok(Response::new(LoginResponse {
            token: output.token,
        }))
    }
}
