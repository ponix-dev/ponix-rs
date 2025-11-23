use std::net::SocketAddr;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing::{error, info};

use ponix_domain::{DeviceService, GatewayService, OrganizationService};
use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceServiceServer;
use ponix_proto_tonic::gateway::v1::tonic::gateway_service_server::GatewayServiceServer;
use ponix_proto_tonic::organization::v1::tonic::organization_service_server::OrganizationServiceServer;

use crate::device_handler::DeviceServiceHandler;
use crate::gateway_handler::GatewayServiceHandler;
use crate::organization_handler::OrganizationServiceHandler;

/// gRPC server configuration
pub struct GrpcServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 50051,
        }
    }
}

/// Run the gRPC server with graceful shutdown
pub async fn run_grpc_server(
    config: GrpcServerConfig,
    device_service: Arc<DeviceService>,
    organization_service: Arc<OrganizationService>,
    gateway_service: Arc<GatewayService>,
    cancellation_token: CancellationToken,
) -> Result<(), anyhow::Error> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid server address");

    info!("Starting gRPC server on {}", addr);

    // Create handlers
    let device_handler = DeviceServiceHandler::new(device_service);
    let organization_handler = OrganizationServiceHandler::new(organization_service);
    let gateway_handler = GatewayServiceHandler::new(gateway_service);

    // Build server with graceful shutdown
    let server = Server::builder()
        .add_service(EndDeviceServiceServer::new(device_handler))
        .add_service(OrganizationServiceServer::new(organization_handler))
        .add_service(GatewayServiceServer::new(gateway_handler))
        .serve_with_shutdown(addr, async move {
            cancellation_token.cancelled().await;
            info!("gRPC server shutdown signal received");
        });

    match server.await {
        Ok(_) => {
            info!("gRPC server stopped gracefully");
            Ok(())
        }
        Err(e) => {
            error!("gRPC server error: {}", e);
            Err(e.into())
        }
    }
}
