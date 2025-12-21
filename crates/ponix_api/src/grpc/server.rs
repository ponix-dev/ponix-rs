//! gRPC server setup for Ponix API.
//!
//! This module builds the Routes for all Ponix API services and delegates
//! to the reusable `common::grpc::run_grpc_server` for actual server execution.

use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::service::Routes;

use crate::domain::{DeviceService, GatewayService, OrganizationService, UserService};
use crate::grpc::{
    DeviceServiceHandler, GatewayServiceHandler, OrganizationServiceHandler, UserServiceHandler,
};
use common::auth::AuthTokenProvider;
use common::grpc::{run_grpc_server, GrpcServerConfig};
use ponix_proto_prost;
use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceServiceServer;
use ponix_proto_tonic::gateway::v1::tonic::gateway_service_server::GatewayServiceServer;
use ponix_proto_tonic::organization::v1::tonic::organization_service_server::OrganizationServiceServer;
use ponix_proto_tonic::user::v1::tonic::user_service_server::UserServiceServer;

/// File descriptor sets for gRPC reflection service.
pub const REFLECTION_DESCRIPTORS: &[&[u8]] = &[
    ponix_proto_prost::end_device::v1::FILE_DESCRIPTOR_SET,
    ponix_proto_prost::organization::v1::FILE_DESCRIPTOR_SET,
    ponix_proto_prost::gateway::v1::FILE_DESCRIPTOR_SET,
    ponix_proto_prost::user::v1::FILE_DESCRIPTOR_SET,
];

/// Build Routes containing all Ponix API services.
pub fn build_ponix_api_routes(
    device_service: Arc<DeviceService>,
    organization_service: Arc<OrganizationService>,
    gateway_service: Arc<GatewayService>,
    user_service: Arc<UserService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
    refresh_token_expiration_days: u64,
    secure_cookies: bool,
) -> Routes {
    // Create handlers
    let device_handler =
        DeviceServiceHandler::new(device_service, auth_token_provider.clone());
    let organization_handler =
        OrganizationServiceHandler::new(organization_service, auth_token_provider.clone());
    let gateway_handler =
        GatewayServiceHandler::new(gateway_service, auth_token_provider);
    let user_handler =
        UserServiceHandler::new(user_service, refresh_token_expiration_days, secure_cookies);

    // Build routes with all services
    let mut builder = Routes::builder();
    builder
        .add_service(EndDeviceServiceServer::new(device_handler))
        .add_service(OrganizationServiceServer::new(organization_handler))
        .add_service(GatewayServiceServer::new(gateway_handler))
        .add_service(UserServiceServer::new(user_handler));
    builder.routes()
}

/// Run the Ponix API gRPC server with graceful shutdown.
///
/// This function builds the service routes and delegates to the reusable
/// `run_grpc_server` from common, which handles:
/// - gRPC-Web support (if enabled in config)
/// - CORS configuration
/// - Logging and tracing middleware
/// - Reflection service
pub async fn run_ponix_grpc_server(
    config: GrpcServerConfig,
    device_service: Arc<DeviceService>,
    organization_service: Arc<OrganizationService>,
    gateway_service: Arc<GatewayService>,
    user_service: Arc<UserService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
    refresh_token_expiration_days: u64,
    secure_cookies: bool,
    cancellation_token: CancellationToken,
) -> Result<(), anyhow::Error> {
    let routes = build_ponix_api_routes(
        device_service,
        organization_service,
        gateway_service,
        user_service,
        auth_token_provider,
        refresh_token_expiration_days,
        secure_cookies,
    );

    run_grpc_server(config, routes, REFLECTION_DESCRIPTORS, cancellation_token).await
}
