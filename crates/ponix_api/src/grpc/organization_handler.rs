use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use crate::domain::OrganizationService;
use ponix_proto_prost::organization::v1::{
    CreateOrganizationRequest, CreateOrganizationResponse, DeleteOrganizationRequest,
    DeleteOrganizationResponse, GetOrganizationRequest, GetOrganizationResponse,
};
use ponix_proto_tonic::organization::v1::tonic::organization_service_server::OrganizationService as OrganizationServiceTrait;

use common::{
    datetime_to_timestamp, domain_error_to_status, to_create_organization_input,
    to_delete_organization_input, to_get_organization_input, to_proto_organization,
};

/// gRPC handler for OrganizationService
/// Handles Proto → Domain mapping and error conversion
pub struct OrganizationServiceHandler {
    domain_service: Arc<OrganizationService>,
}

impl OrganizationServiceHandler {
    pub fn new(domain_service: Arc<OrganizationService>) -> Self {
        Self { domain_service }
    }
}

#[tonic::async_trait]
impl OrganizationServiceTrait for OrganizationServiceHandler {
    async fn create_organization(
        &self,
        request: Request<CreateOrganizationRequest>,
    ) -> Result<Response<CreateOrganizationResponse>, Status> {
        let req = request.into_inner();

        debug!(name = %req.name, "Received CreateOrganization request");

        // Convert proto → domain
        let input = to_create_organization_input(req);

        // Call domain service
        let organization = self
            .domain_service
            .create_organization(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(organization_id = %organization.id, "Organization created successfully");

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

    async fn get_organization(
        &self,
        request: Request<GetOrganizationRequest>,
    ) -> Result<Response<GetOrganizationResponse>, Status> {
        let req = request.into_inner();

        debug!(organization_id = %req.organization_id, "Received GetOrganization request");

        // Convert proto → domain
        let input = to_get_organization_input(req);

        // Call domain service
        let organization = self
            .domain_service
            .get_organization(input)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_organization = to_proto_organization(organization);

        Ok(Response::new(GetOrganizationResponse {
            organization: Some(proto_organization),
        }))
    }

    async fn delete_organization(
        &self,
        request: Request<DeleteOrganizationRequest>,
    ) -> Result<Response<DeleteOrganizationResponse>, Status> {
        let req = request.into_inner();

        debug!(organization_id = %req.organization_id, "Received DeleteOrganization request");

        // Convert proto → domain
        let input = to_delete_organization_input(req);
        let organization_id = input.organization_id.clone();

        // Call domain service
        self.domain_service
            .delete_organization(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(organization_id = %organization_id, "Organization deleted successfully");

        // Return empty response with organization field (required by proto)
        Ok(Response::new(DeleteOrganizationResponse {
            organization: None, // Organization is soft-deleted, not returned
        }))
    }
}
