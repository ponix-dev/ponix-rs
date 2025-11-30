use crate::domain::{DeviceService, GatewayService, OrganizationService};
use crate::grpc::run_ponix_grpc_server;
use common::grpc::GrpcServerConfig;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub struct PonixApi {
    device_service: Arc<DeviceService>,
    organization_service: Arc<OrganizationService>,
    gateway_service: Arc<GatewayService>,
    config: GrpcServerConfig,
}

impl PonixApi {
    pub fn new(
        device_service: Arc<DeviceService>,
        organization_service: Arc<OrganizationService>,
        gateway_service: Arc<GatewayService>,
        config: GrpcServerConfig,
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
            Box::pin(async move {
                run_ponix_grpc_server(
                    self.config,
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
