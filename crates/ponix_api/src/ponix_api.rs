use crate::domain::{DeviceService, GatewayService, OrganizationService};
use crate::grpc::{run_grpc_server, GrpcServerConfig};
use common::grpc::{GrpcLoggingConfig, GrpcTracingConfig};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub struct PonixApiConfig {
    pub grpc_host: String,
    pub grpc_port: u16,
    pub grpc_logging_config: GrpcLoggingConfig,
    pub grpc_tracing_config: GrpcTracingConfig,
}

impl Default for PonixApiConfig {
    fn default() -> Self {
        Self {
            grpc_host: "0.0.0.0".to_string(),
            grpc_port: 50051,
            grpc_logging_config: GrpcLoggingConfig::default(),
            grpc_tracing_config: GrpcTracingConfig::default(),
        }
    }
}

pub struct PonixApi {
    device_service: Arc<DeviceService>,
    organization_service: Arc<OrganizationService>,
    gateway_service: Arc<GatewayService>,
    config: PonixApiConfig,
}

impl PonixApi {
    pub fn new(
        device_service: Arc<DeviceService>,
        organization_service: Arc<OrganizationService>,
        gateway_service: Arc<GatewayService>,
        config: PonixApiConfig,
    ) -> Self {
        debug!("Initializing Ponix API module");
        Self {
            device_service,
            organization_service,
            gateway_service,
            config,
        }
    }

    pub fn into_runner_process(
        self,
    ) -> impl FnOnce(
        CancellationToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>,
    > {
        move |ctx| {
            let grpc_config = GrpcServerConfig {
                host: self.config.grpc_host,
                port: self.config.grpc_port,
                logging_config: self.config.grpc_logging_config,
                tracing_config: self.config.grpc_tracing_config,
            };
            Box::pin(async move {
                run_grpc_server(
                    grpc_config,
                    self.device_service,
                    self.organization_service,
                    self.gateway_service,
                    ctx,
                )
                .await
            })
        }
    }
}
