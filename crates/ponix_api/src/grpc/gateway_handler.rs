use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::GatewayService;
use ponix_proto_prost::gateway::v1::{
    CreateGatewayRequest, CreateGatewayResponse, DeleteGatewayRequest, DeleteGatewayResponse,
    GetGatewayRequest, GetGatewayResponse, ListGatewaysRequest, ListGatewaysResponse,
    UpdateGatewayRequest, UpdateGatewayResponse,
};
use ponix_proto_tonic::gateway::v1::tonic::gateway_service_server::GatewayService as GatewayServiceTrait;

use common::grpc::domain_error_to_status;
use common::proto::{
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
    #[instrument(
        name = "CreateGateway",
        skip(self, request),
        fields(
            organization_id = %request.get_ref().organization_id,
            gateway_name = %request.get_ref().name,
        )
    )]
    async fn create_gateway(
        &self,
        request: Request<CreateGatewayRequest>,
    ) -> Result<Response<CreateGatewayResponse>, Status> {
        let req = request.into_inner();

        let input = to_create_gateway_input(req);

        let gateway = self
            .domain_service
            .create_gateway(input)
            .await
            .map_err(domain_error_to_status)?;

        debug!(gateway_id = %gateway.gateway_id, "Gateway created successfully");

        Ok(Response::new(CreateGatewayResponse {
            gateway: Some(to_proto_gateway(gateway)),
        }))
    }

    #[instrument(
        name = "GetGateway",
        skip(self, request),
        fields(
            gateway_id = %request.get_ref().gateway_id,
            organization_id = %request.get_ref().organization_id
        )
    )]
    async fn get_gateway(
        &self,
        request: Request<GetGatewayRequest>,
    ) -> Result<Response<GetGatewayResponse>, Status> {
        let req = request.into_inner();

        let input = to_get_gateway_input(req);

        let gateway = self
            .domain_service
            .get_gateway(input)
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(GetGatewayResponse {
            gateway: Some(to_proto_gateway(gateway)),
        }))
    }

    #[instrument(
        name = "ListGateways",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id)
    )]
    async fn list_gateways(
        &self,
        request: Request<ListGatewaysRequest>,
    ) -> Result<Response<ListGatewaysResponse>, Status> {
        let req = request.into_inner();

        let input = to_list_gateways_input(req);

        let gateways = self
            .domain_service
            .list_gateways(input)
            .await
            .map_err(domain_error_to_status)?;

        debug!(count = gateways.len(), "Gateways listed successfully");

        let proto_gateways = gateways.into_iter().map(to_proto_gateway).collect();

        Ok(Response::new(ListGatewaysResponse {
            gateways: proto_gateways,
        }))
    }

    #[instrument(
        name = "UpdateGateway",
        skip(self, request),
        fields(
            gateway_id = %request.get_ref().gateway_id,
            organization_id = %request.get_ref().organization_id
        )
    )]
    async fn update_gateway(
        &self,
        request: Request<UpdateGatewayRequest>,
    ) -> Result<Response<UpdateGatewayResponse>, Status> {
        let req = request.into_inner();

        let input = to_update_gateway_input(req);

        let gateway = self
            .domain_service
            .update_gateway(input)
            .await
            .map_err(domain_error_to_status)?;

        debug!(gateway_id = %gateway.gateway_id, "Gateway updated successfully");

        Ok(Response::new(UpdateGatewayResponse {
            gateway: Some(to_proto_gateway(gateway)),
        }))
    }

    #[instrument(
        name = "DeleteGateway",
        skip(self, request),
        fields(
            gateway_id = %request.get_ref().gateway_id,
            organization_id = %request.get_ref().organization_id
        )
    )]
    async fn delete_gateway(
        &self,
        request: Request<DeleteGatewayRequest>,
    ) -> Result<Response<DeleteGatewayResponse>, Status> {
        let req = request.into_inner();
        let gateway_id = req.gateway_id.clone();

        let input = to_delete_gateway_input(req);

        self.domain_service
            .delete_gateway(input)
            .await
            .map_err(domain_error_to_status)?;

        debug!(gateway_id = %gateway_id, "Gateway deleted successfully");

        Ok(Response::new(DeleteGatewayResponse {}))
    }
}
