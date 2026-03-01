use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    CreateDataStreamRequest, DataStreamService, GetDataStreamRequest,
    ListDataStreamsByGatewayRequest, ListDataStreamsRequest,
};
use common::auth::AuthTokenProvider;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::to_proto_data_stream;
use ponix_proto_prost::data_stream::v1::{
    CreateDataStreamRequest as ProtoCreateDataStreamRequest, CreateDataStreamResponse, DataStream,
    GetDataStreamRequest as ProtoGetDataStreamRequest, GetDataStreamResponse,
    GetGatewayDataStreamsRequest, GetGatewayDataStreamsResponse, GetWorkspaceDataStreamsRequest,
    GetWorkspaceDataStreamsResponse,
};
use ponix_proto_tonic::data_stream::v1::tonic::data_stream_service_server::DataStreamService as DataStreamServiceTrait;

/// gRPC handler for DataStreamService
/// Handles Proto -> Domain mapping and error conversion
pub struct DataStreamServiceHandler {
    domain_service: Arc<DataStreamService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl DataStreamServiceHandler {
    pub fn new(
        domain_service: Arc<DataStreamService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
    }
}

#[tonic::async_trait]
impl DataStreamServiceTrait for DataStreamServiceHandler {
    #[instrument(
        name = "CreateDataStream",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            gateway_id = %request.get_ref().gateway_id,
            data_stream_name = %request.get_ref().name,
        )
    )]
    async fn create_data_stream(
        &self,
        request: Request<ProtoCreateDataStreamRequest>,
    ) -> Result<Response<CreateDataStreamResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = CreateDataStreamRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
            definition_id: req.definition_id,
            gateway_id: req.gateway_id,
            name: req.name,
        };

        // Call domain service
        let data_stream = self
            .domain_service
            .create_data_stream(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(data_stream_id = %data_stream.data_stream_id, "Data stream created successfully");

        // Convert domain -> proto
        let proto_data_stream = to_proto_data_stream(data_stream);

        Ok(Response::new(CreateDataStreamResponse {
            data_stream: Some(proto_data_stream),
        }))
    }

    #[instrument(
        name = "GetDataStream",
        skip(self, request),
        fields(
            data_stream_id = %request.get_ref().data_stream_id,
            organization_id = %request.get_ref().organization_id,
            workspace_id = %request.get_ref().workspace_id
        )
    )]
    async fn get_data_stream(
        &self,
        request: Request<ProtoGetDataStreamRequest>,
    ) -> Result<Response<GetDataStreamResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = GetDataStreamRequest {
            user_id: user_context.user_id,
            data_stream_id: req.data_stream_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
        };

        // Call domain service
        let data_stream = self
            .domain_service
            .get_data_stream(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain -> proto
        let proto_data_stream = to_proto_data_stream(data_stream);

        Ok(Response::new(GetDataStreamResponse {
            data_stream: Some(proto_data_stream),
        }))
    }

    #[instrument(
        name = "GetWorkspaceDataStreams",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            workspace_id = %request.get_ref().workspace_id
        )
    )]
    async fn get_workspace_data_streams(
        &self,
        request: Request<GetWorkspaceDataStreamsRequest>,
    ) -> Result<Response<GetWorkspaceDataStreamsResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = ListDataStreamsRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
        };

        // Call domain service
        let data_streams = self
            .domain_service
            .list_data_streams(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(
            count = data_streams.len(),
            "Listed data streams for workspace"
        );

        // Convert domain -> proto
        let proto_data_streams: Vec<DataStream> =
            data_streams.into_iter().map(to_proto_data_stream).collect();

        Ok(Response::new(GetWorkspaceDataStreamsResponse {
            data_streams: proto_data_streams,
        }))
    }

    #[instrument(
        name = "GetGatewayDataStreams",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            gateway_id = %request.get_ref().gateway_id
        )
    )]
    async fn get_gateway_data_streams(
        &self,
        request: Request<GetGatewayDataStreamsRequest>,
    ) -> Result<Response<GetGatewayDataStreamsResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = ListDataStreamsByGatewayRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            gateway_id: req.gateway_id,
        };

        // Call domain service
        let data_streams = self
            .domain_service
            .list_data_streams_by_gateway(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(
            count = data_streams.len(),
            "Listed data streams for gateway"
        );

        // Convert domain -> proto
        let proto_data_streams: Vec<DataStream> =
            data_streams.into_iter().map(to_proto_data_stream).collect();

        Ok(Response::new(GetGatewayDataStreamsResponse {
            data_streams: proto_data_streams,
        }))
    }
}
