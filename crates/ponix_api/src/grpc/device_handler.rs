use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    CreateDeviceRequest, DeviceService, GetDeviceRequest, ListDevicesByGatewayRequest,
    ListDevicesRequest,
};
use common::auth::AuthTokenProvider;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::to_proto_device;
use ponix_proto_prost::end_device::v1::{
    CreateEndDeviceRequest, CreateEndDeviceResponse, EndDevice, GetEndDeviceRequest,
    GetEndDeviceResponse, GetGatewayEndDevicesRequest, GetGatewayEndDevicesResponse,
    GetWorkspaceEndDevicesRequest, GetWorkspaceEndDevicesResponse,
};
use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceService as DeviceServiceTrait;

/// gRPC handler for DeviceService
/// Handles Proto → Domain mapping and error conversion
pub struct DeviceServiceHandler {
    domain_service: Arc<DeviceService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl DeviceServiceHandler {
    pub fn new(
        domain_service: Arc<DeviceService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
    }
}

#[tonic::async_trait]
impl DeviceServiceTrait for DeviceServiceHandler {
    #[instrument(
        name = "CreateEndDevice",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            gateway_id = %request.get_ref().gateway_id,
            device_name = %request.get_ref().name,
        )
    )]
    async fn create_end_device(
        &self,
        request: Request<CreateEndDeviceRequest>,
    ) -> Result<Response<CreateEndDeviceResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = CreateDeviceRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
            definition_id: req.definition_id,
            gateway_id: req.gateway_id,
            name: req.name,
        };

        // Call domain service
        let device = self
            .domain_service
            .create_device(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(device_id = %device.device_id, "Device created successfully");

        // Convert domain → proto
        let proto_device = to_proto_device(device);

        Ok(Response::new(CreateEndDeviceResponse {
            end_device: Some(proto_device),
        }))
    }

    #[instrument(
        name = "GetEndDevice",
        skip(self, request),
        fields(
            device_id = %request.get_ref().device_id,
            organization_id = %request.get_ref().organization_id,
            workspace_id = %request.get_ref().workspace_id
        )
    )]
    async fn get_end_device(
        &self,
        request: Request<GetEndDeviceRequest>,
    ) -> Result<Response<GetEndDeviceResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = GetDeviceRequest {
            user_id: user_context.user_id,
            device_id: req.device_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
        };

        // Call domain service
        let device = self
            .domain_service
            .get_device(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_device = to_proto_device(device);

        Ok(Response::new(GetEndDeviceResponse {
            end_device: Some(proto_device),
        }))
    }

    #[instrument(
        name = "GetWorkspaceEndDevices",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            workspace_id = %request.get_ref().workspace_id
        )
    )]
    async fn get_workspace_end_devices(
        &self,
        request: Request<GetWorkspaceEndDevicesRequest>,
    ) -> Result<Response<GetWorkspaceEndDevicesResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct service request with embedded user_id
        let service_request = ListDevicesRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
        };

        // Call domain service
        let devices = self
            .domain_service
            .list_devices(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(count = devices.len(), "Listed devices for workspace");

        // Convert domain → proto
        let proto_devices: Vec<EndDevice> = devices.into_iter().map(to_proto_device).collect();

        Ok(Response::new(GetWorkspaceEndDevicesResponse {
            end_devices: proto_devices,
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
        let service_request = ListDevicesByGatewayRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            gateway_id: req.gateway_id,
        };

        // Call domain service
        let devices = self
            .domain_service
            .list_devices_by_gateway(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(count = devices.len(), "Listed devices for gateway");

        // Convert domain → proto
        let proto_devices: Vec<EndDevice> = devices.into_iter().map(to_proto_device).collect();

        Ok(Response::new(GetGatewayEndDevicesResponse {
            end_devices: proto_devices,
        }))
    }
}
