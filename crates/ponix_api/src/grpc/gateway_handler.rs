use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    CreateGatewayRequest as DomainCreateRequest, DeleteGatewayRequest as DomainDeleteRequest,
    GatewayService, GetGatewayRequest as DomainGetRequest,
    ListGatewaysRequest as DomainListRequest, UpdateGatewayRequest as DomainUpdateRequest,
};
use common::auth::AuthTokenProvider;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::{
    proto_create_config_to_domain, proto_gateway_type_to_string, proto_update_config_to_domain,
    to_proto_gateway,
};
use ponix_proto_prost::gateway::v1::{
    CreateGatewayRequest, CreateGatewayResponse, DeleteGatewayRequest, DeleteGatewayResponse,
    GetGatewayRequest, GetGatewayResponse, ListGatewaysRequest, ListGatewaysResponse,
    UpdateGatewayRequest, UpdateGatewayResponse,
};
use ponix_proto_tonic::gateway::v1::tonic::gateway_service_server::GatewayService as GatewayServiceTrait;

/// gRPC handler for GatewayService
/// Handles Proto → Domain mapping and error conversion
pub struct GatewayServiceHandler {
    domain_service: Arc<GatewayService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl GatewayServiceHandler {
    pub fn new(
        domain_service: Arc<GatewayService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
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
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Convert proto types before constructing request (to avoid borrow issues)
        let gateway_type = proto_gateway_type_to_string(req.r#type());
        let gateway_config = proto_create_config_to_domain(req.config);

        // Construct domain request
        let service_request = DomainCreateRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            name: req.name,
            gateway_type,
            gateway_config,
        };

        // Call domain service
        let gateway = self
            .domain_service
            .create_gateway(service_request)
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
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainGetRequest {
            user_id: user_context.user_id,
            gateway_id: req.gateway_id,
            organization_id: req.organization_id,
        };

        // Call domain service
        let gateway = self
            .domain_service
            .get_gateway(service_request)
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
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainListRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
        };

        // Call domain service
        let gateways = self
            .domain_service
            .list_gateways(service_request)
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
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Convert gateway_type (only if not 0/unspecified)
        let gateway_type = if req.r#type != 0 {
            Some(proto_gateway_type_to_string(
                ponix_proto_prost::gateway::v1::GatewayType::try_from(req.r#type)
                    .unwrap_or(ponix_proto_prost::gateway::v1::GatewayType::Unspecified),
            ))
        } else {
            None
        };

        // Convert name (only if not empty)
        let name = if req.name.is_empty() {
            None
        } else {
            Some(req.name)
        };

        // Construct domain request directly
        let service_request = DomainUpdateRequest {
            user_id: user_context.user_id,
            gateway_id: req.gateway_id,
            organization_id: req.organization_id,
            name,
            gateway_type,
            gateway_config: proto_update_config_to_domain(req.config),
        };

        // Call domain service
        let gateway = self
            .domain_service
            .update_gateway(service_request)
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
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();
        let gateway_id = req.gateway_id.clone();

        // Construct domain request directly
        let service_request = DomainDeleteRequest {
            user_id: user_context.user_id,
            gateway_id: req.gateway_id,
            organization_id: req.organization_id,
        };

        // Call domain service
        self.domain_service
            .delete_gateway(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(gateway_id = %gateway_id, "Gateway deleted successfully");

        Ok(Response::new(DeleteGatewayResponse {}))
    }
}
