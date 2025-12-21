use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    CreateOrganizationRequest as DomainCreateRequest,
    DeleteOrganizationRequest as DomainDeleteRequest, GetOrganizationRequest as DomainGetRequest,
    GetUserOrganizationsRequest as DomainGetUserOrgsRequest,
    ListOrganizationsRequest as DomainListRequest, OrganizationService,
};
use common::auth::AuthTokenProvider;
use common::domain::DomainError;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::{datetime_to_timestamp, to_proto_organization};
use ponix_proto_prost::organization::v1::{
    CreateOrganizationRequest, CreateOrganizationResponse, DeleteOrganizationRequest,
    DeleteOrganizationResponse, GetOrganizationRequest, GetOrganizationResponse,
    ListOrganizationsRequest, ListOrganizationsResponse, UserOrganizationsRequest,
    UserOrganizationsResponse,
};
use ponix_proto_tonic::organization::v1::tonic::organization_service_server::OrganizationService as OrganizationServiceTrait;

/// gRPC handler for OrganizationService
/// Handles Proto → Domain mapping and error conversion
pub struct OrganizationServiceHandler {
    domain_service: Arc<OrganizationService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl OrganizationServiceHandler {
    pub fn new(
        domain_service: Arc<OrganizationService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
    }
}

#[tonic::async_trait]
impl OrganizationServiceTrait for OrganizationServiceHandler {
    #[instrument(
        name = "CreateOrganization",
        skip(self, request),
        fields(organization_name = %request.get_ref().name)
    )]
    async fn create_organization(
        &self,
        request: Request<CreateOrganizationRequest>,
    ) -> Result<Response<CreateOrganizationResponse>, Status> {
        // Extract user_id from authorization header (mandatory)
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;

        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainCreateRequest {
            user_id: user_context.user_id,
            name: req.name,
        };

        // Call domain service
        let organization = self
            .domain_service
            .create_organization(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(organization_id = %organization.id, "Organization created successfully");

        // Return the created organization details
        Ok(Response::new(CreateOrganizationResponse {
            organization_id: organization.id.clone(),
            name: organization.name.clone(),
            status: if organization.deleted_at.is_some() {
                1
            } else {
                0
            },
            created_at: datetime_to_timestamp(organization.created_at),
        }))
    }

    #[instrument(
        name = "GetOrganization",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id)
    )]
    async fn get_organization(
        &self,
        request: Request<GetOrganizationRequest>,
    ) -> Result<Response<GetOrganizationResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainGetRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
        };

        // Call domain service
        let organization = self
            .domain_service
            .get_organization(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_organization = to_proto_organization(organization);

        Ok(Response::new(GetOrganizationResponse {
            organization: Some(proto_organization),
        }))
    }

    #[instrument(
        name = "DeleteOrganization",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id)
    )]
    async fn delete_organization(
        &self,
        request: Request<DeleteOrganizationRequest>,
    ) -> Result<Response<DeleteOrganizationResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request directly
        let organization_id = req.organization_id.clone();
        let service_request = DomainDeleteRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
        };

        // Call domain service
        self.domain_service
            .delete_organization(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(organization_id = %organization_id, "Organization deleted successfully");

        // Return empty response with organization field (required by proto)
        Ok(Response::new(DeleteOrganizationResponse {
            organization: None, // Organization is soft-deleted, not returned
        }))
    }

    #[instrument(name = "ListOrganizations", skip(self, _request))]
    async fn list_organizations(
        &self,
        _request: Request<ListOrganizationsRequest>,
    ) -> Result<Response<ListOrganizationsResponse>, Status> {
        // Construct domain request directly
        let service_request = DomainListRequest {};

        // Call domain service
        let organizations = self
            .domain_service
            .list_organizations(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_organizations: Vec<_> = organizations
            .into_iter()
            .map(to_proto_organization)
            .collect();

        debug!(
            count = proto_organizations.len(),
            "Organizations listed successfully"
        );

        Ok(Response::new(ListOrganizationsResponse {
            organizations: proto_organizations,
        }))
    }

    #[instrument(
        name = "UserOrganizations",
        skip(self, request),
        fields(user_id = %request.get_ref().user_id)
    )]
    async fn user_organizations(
        &self,
        request: Request<UserOrganizationsRequest>,
    ) -> Result<Response<UserOrganizationsResponse>, Status> {
        // Extract user_id from authorization header (mandatory for authenticated endpoints)
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let authenticated_user_id = user_context.user_id;

        let req = request.into_inner();

        // Authorization check: user can only retrieve their own organizations
        if authenticated_user_id != req.user_id {
            return Err(domain_error_to_status(DomainError::PermissionDenied(
                "Cannot access organizations for another user".to_string(),
            )));
        }

        // Construct domain request directly
        let service_request = DomainGetUserOrgsRequest {
            user_id: req.user_id,
        };

        // Call domain service
        let organizations = self
            .domain_service
            .get_user_organizations(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_organizations: Vec<_> = organizations
            .into_iter()
            .map(to_proto_organization)
            .collect();

        debug!(
            count = proto_organizations.len(),
            "User organizations retrieved successfully"
        );

        Ok(Response::new(UserOrganizationsResponse {
            organizations: proto_organizations,
        }))
    }
}
