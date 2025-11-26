use std::net::SocketAddr;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing::{error, info};

use crate::domain::{DeviceService, GatewayService, OrganizationService};
use ponix_proto_prost;
use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceServiceServer;
use ponix_proto_tonic::gateway::v1::tonic::gateway_service_server::GatewayServiceServer;
use ponix_proto_tonic::organization::v1::tonic::organization_service_server::OrganizationServiceServer;

use crate::grpc::device_handler::DeviceServiceHandler;
use crate::grpc::gateway_handler::GatewayServiceHandler;
use crate::grpc::organization_handler::OrganizationServiceHandler;

/// Build reflection service descriptor for all services
fn build_reflection_service(
) -> tonic_reflection::server::ServerReflectionServer<impl tonic_reflection::server::ServerReflection>
{
    tonic_reflection::server::Builder::configure()
        // Register Google well-known types
        .register_encoded_file_descriptor_set(protoc_wkt::google::protobuf::FILE_DESCRIPTOR_SET)
        // Register our service descriptors
        .register_encoded_file_descriptor_set(
            ponix_proto_prost::end_device::v1::FILE_DESCRIPTOR_SET,
        )
        .register_encoded_file_descriptor_set(
            ponix_proto_prost::organization::v1::FILE_DESCRIPTOR_SET,
        )
        .register_encoded_file_descriptor_set(ponix_proto_prost::gateway::v1::FILE_DESCRIPTOR_SET)
        .build_v1()
        .expect("Failed to build reflection service")
}

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

    // Build reflection service
    let reflection_service = build_reflection_service();

    // Build server with graceful shutdown
    let server = Server::builder()
        .add_service(reflection_service)
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
