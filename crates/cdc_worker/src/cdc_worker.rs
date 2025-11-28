use crate::domain::{CdcConfig, CdcProcess, EntityConfig};
use common::nats::NatsClient;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub struct CdcWorkerConfig {
    pub cdc_config: CdcConfig,
    pub entity_configs: Vec<EntityConfig>,
}

pub struct CdcWorker {
    cdc_config: CdcConfig,
    entity_configs: Vec<EntityConfig>,
    nats_client: Arc<NatsClient>,
}

impl CdcWorker {
    pub fn new(nats_client: Arc<NatsClient>, config: CdcWorkerConfig) -> Self {
        debug!("initializing CDC worker module");
        Self {
            cdc_config: config.cdc_config,
            entity_configs: config.entity_configs,
            nats_client,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn into_runner_process(
        self,
    ) -> Box<
        dyn FnOnce(
                CancellationToken,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>,
            > + Send,
    > {
        Box::new({
            let cdc_config = self.cdc_config;
            let nats_client = self.nats_client;
            let entity_configs = self.entity_configs;
            move |ctx| {
                let cdc_process = CdcProcess::new(cdc_config, nats_client, entity_configs, ctx);
                Box::pin(async move { cdc_process.run().await })
            }
        })
    }
}
