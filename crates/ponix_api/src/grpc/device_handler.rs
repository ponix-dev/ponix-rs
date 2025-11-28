use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info, instrument};

use crate::domain::DeviceService;
use ponix_proto_prost::end_device::v1::{
    CreateEndDeviceRequest, CreateEndDeviceResponse, EndDevice, GetEndDeviceRequest,
    GetEndDeviceResponse, ListEndDevicesRequest, ListEndDevicesResponse,
};
use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceService as DeviceServiceTrait;

use common::grpc::{domain_error_to_status, RecordGrpcStatus};
use common::proto::{
    to_create_device_input, to_get_device_input, to_list_devices_input, to_proto_device,
};

/// gRPC handler for DeviceService
/// Handles Proto → Domain mapping and error conversion
pub struct DeviceServiceHandler {
    domain_service: Arc<DeviceService>,
}

impl DeviceServiceHandler {
    pub fn new(domain_service: Arc<DeviceService>) -> Self {
        Self { domain_service }
    }
}

#[tonic::async_trait]
impl DeviceServiceTrait for DeviceServiceHandler {
    #[instrument(
        name = "CreateEndDevice",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            device_name = %request.get_ref().name,
            rpc.grpc.status_code = tracing::field::Empty
        )
    )]
    async fn create_end_device(
        &self,
        request: Request<CreateEndDeviceRequest>,
    ) -> Result<Response<CreateEndDeviceResponse>, Status> {
        let req = request.into_inner();

        // Convert proto → domain
        let input = to_create_device_input(req);

        // Call domain service
        let device = self
            .domain_service
            .create_device(input)
            .await
            .map_err(domain_error_to_status)?;

        debug!(device_id = %device.device_id, "Device created successfully");

        // Convert domain → proto
        let proto_device = to_proto_device(device);

        Ok(Response::new(CreateEndDeviceResponse {
            end_device: Some(proto_device),
        }))
        .record_status()
    }

    #[instrument(
        name = "GetEndDevice",
        skip(self, request),
        fields(
            device_id = %request.get_ref().device_id,
            rpc.grpc.status_code = tracing::field::Empty
        )
    )]
    async fn get_end_device(
        &self,
        request: Request<GetEndDeviceRequest>,
    ) -> Result<Response<GetEndDeviceResponse>, Status> {
        let req = request.into_inner();

        // Convert proto → domain
        let input = to_get_device_input(req);

        // Call domain service
        let device = self
            .domain_service
            .get_device(input)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_device = to_proto_device(device);

        Ok(Response::new(GetEndDeviceResponse {
            end_device: Some(proto_device),
        }))
        .record_status()
    }

    #[instrument(
        name = "ListEndDevices",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            rpc.grpc.status_code = tracing::field::Empty
        )
    )]
    async fn list_end_devices(
        &self,
        request: Request<ListEndDevicesRequest>,
    ) -> Result<Response<ListEndDevicesResponse>, Status> {
        let req = request.into_inner();

        // Convert proto → domain
        let input = to_list_devices_input(req);

        // Call domain service
        let devices = self
            .domain_service
            .list_devices(input)
            .await
            .map_err(domain_error_to_status)?;

        debug!(count = devices.len(), "Listed devices");

        // Convert domain → proto
        let proto_devices: Vec<EndDevice> = devices.into_iter().map(to_proto_device).collect();

        Ok(Response::new(ListEndDevicesResponse {
            end_devices: proto_devices,
        }))
        .record_status()
    }
}
