use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    CreateEndDeviceDefinitionRequest, DeleteEndDeviceDefinitionRequest,
    EndDeviceDefinitionService, GetEndDeviceDefinitionRequest,
    ListEndDeviceDefinitionsRequest, UpdateEndDeviceDefinitionRequest,
};
use common::auth::AuthTokenProvider;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::to_proto_end_device_definition;
use ponix_proto_prost::end_device::v1::{
    CreateEndDeviceDefinitionRequest as ProtoCreateRequest,
    CreateEndDeviceDefinitionResponse, DeleteEndDeviceDefinitionRequest as ProtoDeleteRequest,
    DeleteEndDeviceDefinitionResponse, EndDeviceDefinition,
    GetEndDeviceDefinitionRequest as ProtoGetRequest, GetEndDeviceDefinitionResponse,
    ListEndDeviceDefinitionsRequest as ProtoListRequest, ListEndDeviceDefinitionsResponse,
    UpdateEndDeviceDefinitionRequest as ProtoUpdateRequest, UpdateEndDeviceDefinitionResponse,
};
use ponix_proto_tonic::end_device::v1::tonic::end_device_definition_service_server::EndDeviceDefinitionService as EndDeviceDefinitionServiceTrait;

/// gRPC handler for EndDeviceDefinitionService
pub struct EndDeviceDefinitionServiceHandler {
    domain_service: Arc<EndDeviceDefinitionService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl EndDeviceDefinitionServiceHandler {
    pub fn new(
        domain_service: Arc<EndDeviceDefinitionService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
    }
}

#[tonic::async_trait]
impl EndDeviceDefinitionServiceTrait for EndDeviceDefinitionServiceHandler {
    #[instrument(
        name = "CreateEndDeviceDefinition",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id, name = %request.get_ref().name)
    )]
    async fn create_end_device_definition(
        &self,
        request: Request<ProtoCreateRequest>,
    ) -> Result<Response<CreateEndDeviceDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = CreateEndDeviceDefinitionRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            name: req.name,
            json_schema: req.json_schema,
            payload_conversion: req.payload_conversion,
        };

        let definition = self
            .domain_service
            .create_definition(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(id = %definition.id, "Definition created successfully");

        Ok(Response::new(CreateEndDeviceDefinitionResponse {
            end_device_definition: Some(to_proto_end_device_definition(definition)),
        }))
    }

    #[instrument(
        name = "GetEndDeviceDefinition",
        skip(self, request),
        fields(id = %request.get_ref().id, organization_id = %request.get_ref().organization_id)
    )]
    async fn get_end_device_definition(
        &self,
        request: Request<ProtoGetRequest>,
    ) -> Result<Response<GetEndDeviceDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = GetEndDeviceDefinitionRequest {
            user_id: user_context.user_id,
            id: req.id,
            organization_id: req.organization_id,
        };

        let definition = self
            .domain_service
            .get_definition(service_request)
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(GetEndDeviceDefinitionResponse {
            end_device_definition: Some(to_proto_end_device_definition(definition)),
        }))
    }

    #[instrument(
        name = "UpdateEndDeviceDefinition",
        skip(self, request),
        fields(id = %request.get_ref().id, organization_id = %request.get_ref().organization_id)
    )]
    async fn update_end_device_definition(
        &self,
        request: Request<ProtoUpdateRequest>,
    ) -> Result<Response<UpdateEndDeviceDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = UpdateEndDeviceDefinitionRequest {
            user_id: user_context.user_id,
            id: req.id,
            organization_id: req.organization_id,
            name: req.name,
            json_schema: req.json_schema,
            payload_conversion: req.payload_conversion,
        };

        let definition = self
            .domain_service
            .update_definition(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(id = %definition.id, "Definition updated successfully");

        Ok(Response::new(UpdateEndDeviceDefinitionResponse {
            end_device_definition: Some(to_proto_end_device_definition(definition)),
        }))
    }

    #[instrument(
        name = "DeleteEndDeviceDefinition",
        skip(self, request),
        fields(id = %request.get_ref().id, organization_id = %request.get_ref().organization_id)
    )]
    async fn delete_end_device_definition(
        &self,
        request: Request<ProtoDeleteRequest>,
    ) -> Result<Response<DeleteEndDeviceDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = DeleteEndDeviceDefinitionRequest {
            user_id: user_context.user_id,
            id: req.id,
            organization_id: req.organization_id,
        };

        self.domain_service
            .delete_definition(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!("Definition deleted successfully");

        Ok(Response::new(DeleteEndDeviceDefinitionResponse {}))
    }

    #[instrument(
        name = "ListEndDeviceDefinitions",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id)
    )]
    async fn list_end_device_definitions(
        &self,
        request: Request<ProtoListRequest>,
    ) -> Result<Response<ListEndDeviceDefinitionsResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = ListEndDeviceDefinitionsRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
        };

        let definitions = self
            .domain_service
            .list_definitions(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(count = definitions.len(), "Listed definitions");

        let proto_definitions: Vec<EndDeviceDefinition> = definitions
            .into_iter()
            .map(to_proto_end_device_definition)
            .collect();

        Ok(Response::new(ListEndDeviceDefinitionsResponse {
            end_device_definitions: proto_definitions,
        }))
    }
}
