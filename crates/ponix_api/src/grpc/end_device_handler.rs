use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    CreateEndDeviceRequest, EndDeviceService, GetEndDeviceRequest, ListEndDevicesByGatewayRequest,
    ListEndDevicesRequest,
};
use common::auth::AuthTokenProvider;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::to_proto_end_device;
use ponix_proto_prost::end_device::v1::{
    CreateEndDeviceRequest as ProtoCreateEndDeviceRequest, CreateEndDeviceResponse, EndDevice,
    GetEndDeviceRequest as ProtoGetEndDeviceRequest, GetEndDeviceResponse,
    GetGatewayEndDevicesRequest, GetGatewayEndDevicesResponse,
    ListEndDevicesRequest as ProtoListEndDevicesRequest, ListEndDevicesResponse,
};
use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceService as EndDeviceServiceTrait;

/// gRPC handler for EndDeviceService
/// Handles Proto -> Domain mapping and error conversion
pub struct EndDeviceServiceHandler {
    domain_service: Arc<EndDeviceService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl EndDeviceServiceHandler {
    pub fn new(
        domain_service: Arc<EndDeviceService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
    }
}

#[tonic::async_trait]
impl EndDeviceServiceTrait for EndDeviceServiceHandler {
    #[instrument(
        name = "CreateEndDevice",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            gateway_id = %request.get_ref().gateway_id,
            end_device_name = %request.get_ref().name,
        )
    )]
    async fn create_end_device(
        &self,
        request: Request<ProtoCreateEndDeviceRequest>,
    ) -> Result<Response<CreateEndDeviceResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = CreateEndDeviceRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            definition_id: req.definition_id,
            gateway_id: req.gateway_id,
            name: req.name,
        };

        // Call domain service
        let end_device = self
            .domain_service
            .create_end_device(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(end_device_id = %end_device.end_device_id, "End device created successfully");

        // Convert domain -> proto
        let proto_end_device = to_proto_end_device(end_device);

        Ok(Response::new(CreateEndDeviceResponse {
            end_device: Some(proto_end_device),
        }))
    }

    #[instrument(
        name = "GetEndDevice",
        skip(self, request),
        fields(
            end_device_id = %request.get_ref().end_device_id,
            organization_id = %request.get_ref().organization_id,
        )
    )]
    async fn get_end_device(
        &self,
        request: Request<ProtoGetEndDeviceRequest>,
    ) -> Result<Response<GetEndDeviceResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = GetEndDeviceRequest {
            user_id: user_context.user_id,
            end_device_id: req.end_device_id,
            organization_id: req.organization_id,
        };

        // Call domain service
        let end_device = self
            .domain_service
            .get_end_device(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain -> proto
        let proto_end_device = to_proto_end_device(end_device);

        Ok(Response::new(GetEndDeviceResponse {
            end_device: Some(proto_end_device),
        }))
    }

    #[instrument(
        name = "ListEndDevices",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
        )
    )]
    async fn list_end_devices(
        &self,
        request: Request<ProtoListEndDevicesRequest>,
    ) -> Result<Response<ListEndDevicesResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = ListEndDevicesRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
        };

        // Call domain service
        let end_devices = self
            .domain_service
            .list_end_devices(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(
            count = end_devices.len(),
            "Listed end devices for organization"
        );

        // Convert domain -> proto
        let proto_end_devices: Vec<EndDevice> =
            end_devices.into_iter().map(to_proto_end_device).collect();

        Ok(Response::new(ListEndDevicesResponse {
            end_devices: proto_end_devices,
        }))
    }

    #[instrument(
        name = "GetGatewayEndDevices",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            gateway_id = %request.get_ref().gateway_id
        )
    )]
    async fn get_gateway_end_devices(
        &self,
        request: Request<GetGatewayEndDevicesRequest>,
    ) -> Result<Response<GetGatewayEndDevicesResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = ListEndDevicesByGatewayRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            gateway_id: req.gateway_id,
        };

        // Call domain service
        let end_devices = self
            .domain_service
            .list_end_devices_by_gateway(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(count = end_devices.len(), "Listed end devices for gateway");

        // Convert domain -> proto
        let proto_end_devices: Vec<EndDevice> =
            end_devices.into_iter().map(to_proto_end_device).collect();

        Ok(Response::new(GetGatewayEndDevicesResponse {
            end_devices: proto_end_devices,
        }))
    }
}
