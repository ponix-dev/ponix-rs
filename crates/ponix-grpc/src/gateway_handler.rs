use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use ponix_domain::GatewayService;
use ponix_proto_prost::gateway::v1::{
    CreateGatewayRequest, CreateGatewayResponse, DeleteGatewayRequest, DeleteGatewayResponse,
    GetGatewayRequest, GetGatewayResponse, ListGatewaysRequest, ListGatewaysResponse,
    UpdateGatewayRequest, UpdateGatewayResponse,
};
use ponix_proto_tonic::gateway::v1::tonic::gateway_service_server::GatewayService as GatewayServiceTrait;

use crate::error::domain_error_to_status;
use crate::gateway_conversions::{
    to_create_gateway_input, to_delete_gateway_input, to_get_gateway_input, to_list_gateways_input,
    to_proto_gateway, to_update_gateway_input,
};

/// gRPC handler for GatewayService
/// Handles Proto → Domain mapping and error conversion
pub struct GatewayServiceHandler {
    domain_service: Arc<GatewayService>,
}

impl GatewayServiceHandler {
    pub fn new(domain_service: Arc<GatewayService>) -> Self {
        Self { domain_service }
    }
}

#[tonic::async_trait]
impl GatewayServiceTrait for GatewayServiceHandler {
    async fn create_gateway(
        &self,
        request: Request<CreateGatewayRequest>,
    ) -> Result<Response<CreateGatewayResponse>, Status> {
        let req = request.into_inner();
        debug!(
            organization_id = %req.organization_id,
            name = %req.name,
            "Received CreateGateway request"
        );

        let input = to_create_gateway_input(req);

        let gateway = self
            .domain_service
            .create_gateway(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(gateway_id = %gateway.gateway_id, "Gateway created successfully");

        Ok(Response::new(CreateGatewayResponse {
            gateway: Some(to_proto_gateway(gateway)),
        }))
    }

    async fn get_gateway(
        &self,
        request: Request<GetGatewayRequest>,
    ) -> Result<Response<GetGatewayResponse>, Status> {
        let req = request.into_inner();
        debug!(gateway_id = %req.gateway_id, "Received GetGateway request");

        let input = to_get_gateway_input(req);

        let gateway = self
            .domain_service
            .get_gateway(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(gateway_id = %gateway.gateway_id, "Gateway retrieved successfully");

        Ok(Response::new(GetGatewayResponse {
            gateway: Some(to_proto_gateway(gateway)),
        }))
    }

    async fn list_gateways(
        &self,
        request: Request<ListGatewaysRequest>,
    ) -> Result<Response<ListGatewaysResponse>, Status> {
        let req = request.into_inner();
        debug!(
            organization_id = %req.organization_id,
            "Received ListGateways request"
        );

        let input = to_list_gateways_input(req);

        let gateways = self
            .domain_service
            .list_gateways(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(count = gateways.len(), "Gateways listed successfully");

        let proto_gateways = gateways.into_iter().map(to_proto_gateway).collect();

        Ok(Response::new(ListGatewaysResponse {
            gateways: proto_gateways,
        }))
    }

    async fn update_gateway(
        &self,
        request: Request<UpdateGatewayRequest>,
    ) -> Result<Response<UpdateGatewayResponse>, Status> {
        let req = request.into_inner();
        debug!(gateway_id = %req.gateway_id, "Received UpdateGateway request");

        let input = to_update_gateway_input(req);

        let gateway = self
            .domain_service
            .update_gateway(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(gateway_id = %gateway.gateway_id, "Gateway updated successfully");

        Ok(Response::new(UpdateGatewayResponse {
            gateway: Some(to_proto_gateway(gateway)),
        }))
    }

    async fn delete_gateway(
        &self,
        request: Request<DeleteGatewayRequest>,
    ) -> Result<Response<DeleteGatewayResponse>, Status> {
        let req = request.into_inner();
        let gateway_id = req.gateway_id.clone();
        debug!(gateway_id = %gateway_id, "Received DeleteGateway request");

        let input = to_delete_gateway_input(req);

        self.domain_service
            .delete_gateway(input)
            .await
            .map_err(domain_error_to_status)?;

        info!(gateway_id = %gateway_id, "Gateway deleted successfully");

        Ok(Response::new(DeleteGatewayResponse {}))
    }
}
