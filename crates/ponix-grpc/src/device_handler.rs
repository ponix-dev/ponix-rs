use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use ponix_domain::DeviceService;
use ponix_proto_prost::end_device::v1::{
    CreateEndDeviceRequest, CreateEndDeviceResponse, EndDevice, GetEndDeviceRequest,
    GetEndDeviceResponse, ListEndDevicesRequest, ListEndDevicesResponse,
};
use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceService as DeviceServiceTrait;

use crate::conversions::{to_create_device_input, to_get_device_input, to_list_devices_input, to_proto_device};
use crate::error::domain_error_to_status;

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
    async fn create_end_device(
        &self,
        request: Request<CreateEndDeviceRequest>,
    ) -> Result<Response<CreateEndDeviceResponse>, Status> {
        let req = request.into_inner();

        debug!(
            organization_id = %req.organization_id,
            name = %req.name,
            "Received CreateDevice request"
        );

        // Convert proto → domain
        let input = to_create_device_input(req);

        // Call domain service
        let device = self
            .domain_service
            .create_device(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(device_id = %device.device_id, "Device created successfully");

        // Convert domain → proto
        let proto_device = to_proto_device(device);

        Ok(Response::new(CreateEndDeviceResponse {
            end_device: Some(proto_device),
        }))
    }

    async fn get_end_device(
        &self,
        request: Request<GetEndDeviceRequest>,
    ) -> Result<Response<GetEndDeviceResponse>, Status> {
        let req = request.into_inner();

        debug!(device_id = %req.device_id, "Received GetDevice request");

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
    }

    async fn list_end_devices(
        &self,
        request: Request<ListEndDevicesRequest>,
    ) -> Result<Response<ListEndDevicesResponse>, Status> {
        let req = request.into_inner();

        debug!(organization_id = %req.organization_id, "Received ListDevices request");

        // Convert proto → domain
        let input = to_list_devices_input(req);

        // Call domain service
        let devices = self
            .domain_service
            .list_devices(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(count = devices.len(), "Listed devices");

        // Convert domain → proto
        let proto_devices: Vec<EndDevice> = devices.into_iter().map(to_proto_device).collect();

        Ok(Response::new(ListEndDevicesResponse {
            end_devices: proto_devices,
        }))
    }
}
