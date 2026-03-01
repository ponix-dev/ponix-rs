use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    CreateDataStreamDefinitionRequest, DataStreamDefinitionService,
    DeleteDataStreamDefinitionRequest, GetDataStreamDefinitionRequest,
    ListDataStreamDefinitionsRequest, UpdateDataStreamDefinitionRequest,
};
use common::auth::AuthTokenProvider;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::{from_proto_payload_contract, to_proto_data_stream_definition};
use ponix_proto_prost::data_stream::v1::{
    CreateDataStreamDefinitionRequest as ProtoCreateRequest, CreateDataStreamDefinitionResponse,
    DataStreamDefinition, DeleteDataStreamDefinitionRequest as ProtoDeleteRequest,
    DeleteDataStreamDefinitionResponse, GetDataStreamDefinitionRequest as ProtoGetRequest,
    GetDataStreamDefinitionResponse, ListDataStreamDefinitionsRequest as ProtoListRequest,
    ListDataStreamDefinitionsResponse, UpdateDataStreamDefinitionRequest as ProtoUpdateRequest,
    UpdateDataStreamDefinitionResponse,
};
use ponix_proto_tonic::data_stream::v1::tonic::data_stream_definition_service_server::DataStreamDefinitionService as DataStreamDefinitionServiceTrait;

/// gRPC handler for DataStreamDefinitionService
pub struct DataStreamDefinitionServiceHandler {
    domain_service: Arc<DataStreamDefinitionService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl DataStreamDefinitionServiceHandler {
    pub fn new(
        domain_service: Arc<DataStreamDefinitionService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
    }
}

#[tonic::async_trait]
impl DataStreamDefinitionServiceTrait for DataStreamDefinitionServiceHandler {
    #[instrument(
        name = "CreateDataStreamDefinition",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id, name = %request.get_ref().name)
    )]
    async fn create_data_stream_definition(
        &self,
        request: Request<ProtoCreateRequest>,
    ) -> Result<Response<CreateDataStreamDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = CreateDataStreamDefinitionRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            name: req.name,
            contracts: req
                .contracts
                .into_iter()
                .map(from_proto_payload_contract)
                .collect(),
        };

        let definition = self
            .domain_service
            .create_definition(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(id = %definition.id, "Definition created successfully");

        Ok(Response::new(CreateDataStreamDefinitionResponse {
            data_stream_definition: Some(to_proto_data_stream_definition(definition)),
        }))
    }

    #[instrument(
        name = "GetDataStreamDefinition",
        skip(self, request),
        fields(id = %request.get_ref().id, organization_id = %request.get_ref().organization_id)
    )]
    async fn get_data_stream_definition(
        &self,
        request: Request<ProtoGetRequest>,
    ) -> Result<Response<GetDataStreamDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = GetDataStreamDefinitionRequest {
            user_id: user_context.user_id,
            id: req.id,
            organization_id: req.organization_id,
        };

        let definition = self
            .domain_service
            .get_definition(service_request)
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(GetDataStreamDefinitionResponse {
            data_stream_definition: Some(to_proto_data_stream_definition(definition)),
        }))
    }

    #[instrument(
        name = "UpdateDataStreamDefinition",
        skip(self, request),
        fields(id = %request.get_ref().id, organization_id = %request.get_ref().organization_id)
    )]
    async fn update_data_stream_definition(
        &self,
        request: Request<ProtoUpdateRequest>,
    ) -> Result<Response<UpdateDataStreamDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Protobuf cannot distinguish "field not set" from "empty repeated field", so
        // an empty contracts list means "don't update contracts". At least one contract is
        // always required and enforced by both the domain service validation and the DB
        // CHECK constraint (contracts_non_empty).
        let contracts = if req.contracts.is_empty() {
            None
        } else {
            Some(
                req.contracts
                    .into_iter()
                    .map(from_proto_payload_contract)
                    .collect(),
            )
        };

        let service_request = UpdateDataStreamDefinitionRequest {
            user_id: user_context.user_id,
            id: req.id,
            organization_id: req.organization_id,
            name: req.name,
            contracts,
        };

        let definition = self
            .domain_service
            .update_definition(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(id = %definition.id, "Definition updated successfully");

        Ok(Response::new(UpdateDataStreamDefinitionResponse {
            data_stream_definition: Some(to_proto_data_stream_definition(definition)),
        }))
    }

    #[instrument(
        name = "DeleteDataStreamDefinition",
        skip(self, request),
        fields(id = %request.get_ref().id, organization_id = %request.get_ref().organization_id)
    )]
    async fn delete_data_stream_definition(
        &self,
        request: Request<ProtoDeleteRequest>,
    ) -> Result<Response<DeleteDataStreamDefinitionResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = DeleteDataStreamDefinitionRequest {
            user_id: user_context.user_id,
            id: req.id,
            organization_id: req.organization_id,
        };

        self.domain_service
            .delete_definition(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!("Definition deleted successfully");

        Ok(Response::new(DeleteDataStreamDefinitionResponse {}))
    }

    #[instrument(
        name = "ListDataStreamDefinitions",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id)
    )]
    async fn list_data_stream_definitions(
        &self,
        request: Request<ProtoListRequest>,
    ) -> Result<Response<ListDataStreamDefinitionsResponse>, Status> {
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        let service_request = ListDataStreamDefinitionsRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
        };

        let definitions = self
            .domain_service
            .list_definitions(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(count = definitions.len(), "Listed definitions");

        let proto_definitions: Vec<DataStreamDefinition> = definitions
            .into_iter()
            .map(to_proto_data_stream_definition)
            .collect();

        Ok(Response::new(ListDataStreamDefinitionsResponse {
            data_stream_definitions: proto_definitions,
        }))
    }
}
