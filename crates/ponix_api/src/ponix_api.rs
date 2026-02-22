use crate::grpc::{run_ponix_grpc_server, PonixApiServices};
use common::grpc::GrpcServerConfig;
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub struct PonixApi {
    services: PonixApiServices,
    config: GrpcServerConfig,
}

impl PonixApi {
    pub fn new(services: PonixApiServices, config: GrpcServerConfig) -> Self {
        debug!("Initializing Ponix API module");
        Self { services, config }
    }

    pub fn into_runner_process(
        self,
    ) -> impl FnOnce(
        CancellationToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>,
    > {
        move |ctx| {
            Box::pin(async move { run_ponix_grpc_server(self.config, self.services, ctx).await })
        }
    }
}
