use std::net::SocketAddr;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing::{error, info};

use ponix_domain::DeviceService;
use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceServiceServer;

use crate::device_handler::DeviceServiceHandler;

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
    domain_service: Arc<DeviceService>,
    cancellation_token: CancellationToken,
) -> Result<(), anyhow::Error> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid server address");

    info!("Starting gRPC server on {}", addr);

    // Create handler
    let handler = DeviceServiceHandler::new(domain_service);

    // Build server with graceful shutdown
    let server = Server::builder()
        .add_service(EndDeviceServiceServer::new(handler))
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
