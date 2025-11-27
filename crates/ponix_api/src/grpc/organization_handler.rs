use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{info, instrument};

use crate::domain::OrganizationService;
use ponix_proto_prost::organization::v1::{
    CreateOrganizationRequest, CreateOrganizationResponse, DeleteOrganizationRequest,
    DeleteOrganizationResponse, GetOrganizationRequest, GetOrganizationResponse,
};
use ponix_proto_tonic::organization::v1::tonic::organization_service_server::OrganizationService as OrganizationServiceTrait;

use common::grpc::{domain_error_to_status, RecordGrpcStatus};
use common::proto::{
    datetime_to_timestamp, to_create_organization_input, to_delete_organization_input,
    to_get_organization_input, to_proto_organization,
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
    #[instrument(
        name = "CreateOrganization",
        skip(self, request),
        fields(
            organization_name = %request.get_ref().name,
            rpc.grpc.status_code = tracing::field::Empty
        )
    )]
    async fn create_organization(
        &self,
        request: Request<CreateOrganizationRequest>,
    ) -> Result<Response<CreateOrganizationResponse>, Status> {
        let req = request.into_inner();

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
        .record_status()
    }

    #[instrument(
        name = "GetOrganization",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            rpc.grpc.status_code = tracing::field::Empty
        )
    )]
    async fn get_organization(
        &self,
        request: Request<GetOrganizationRequest>,
    ) -> Result<Response<GetOrganizationResponse>, Status> {
        let req = request.into_inner();

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
        .record_status()
    }

    #[instrument(
        name = "DeleteOrganization",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            rpc.grpc.status_code = tracing::field::Empty
        )
    )]
    async fn delete_organization(
        &self,
        request: Request<DeleteOrganizationRequest>,
    ) -> Result<Response<DeleteOrganizationResponse>, Status> {
        let req = request.into_inner();

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
        .record_status()
    }
}
