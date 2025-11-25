use crate::domain::{DeviceService, GatewayService, OrganizationService};
use crate::grpc::{run_grpc_server, GrpcServerConfig};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct PonixApiConfig {
    pub grpc_host: String,
    pub grpc_port: u16,
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
        info!("Initializing Ponix API module");
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
