use crate::cdc::{
    config::CdcConfig, gateway_converter::GatewayConverter, nats_sink::NatsSink,
    traits::EntityConfig,
};
use anyhow::Result;
use etl::config::{BatchConfig, PgConnectionConfig, PipelineConfig, TlsConfig};
use etl::pipeline::Pipeline;
use etl::store::both::memory::MemoryStore;
use ponix_nats::NatsClient;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub struct CdcProcess {
    config: CdcConfig,
    nats_client: Arc<NatsClient>,
    cancellation_token: CancellationToken,
}

impl CdcProcess {
    pub fn new(
        config: CdcConfig,
        nats_client: Arc<NatsClient>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            config,
            nats_client,
            cancellation_token,
        }
    }

    pub async fn run(self) -> Result<()> {
        info!("Starting CDC process");

        // Create NATS publisher
        let publisher = self.nats_client.create_publisher_client();

        // Configure gateway CDC
        let gateway_config = EntityConfig {
            entity_name: "gateway".to_string(),
            table_name: "gateways".to_string(),
            converter: Box::new(GatewayConverter::new()),
        };

        // Create NATS sink
        let destination = NatsSink::new(publisher, vec![gateway_config]);

        // Create in-memory store for state/schema management
        let store = MemoryStore::new();

        // Configure PostgreSQL connection
        let pg_config = PgConnectionConfig {
            host: self.config.pg_host.clone(),
            port: self.config.pg_port,
            name: self.config.pg_database.clone(),
            username: self.config.pg_user.clone(),
            password: Some(self.config.pg_password.clone().into()),
            tls: TlsConfig {
                enabled: false,
                trusted_root_certs: String::new(),
            },
        };

        // Configure ETL pipeline
        let pipeline_config = PipelineConfig {
            id: 1,
            publication_name: self.config.publication_name.clone(),
            pg_connection: pg_config,
            batch: BatchConfig {
                max_size: self.config.batch_size,
                max_fill_ms: self.config.batch_timeout_ms,
            },
            table_error_retry_delay_ms: self.config.retry_delay_ms,
            table_error_retry_max_attempts: self.config.max_retry_attempts,
            max_table_sync_workers: 4,
        };

        // Create pipeline
        let mut pipeline = Pipeline::new(pipeline_config, store, destination);

        // Run until cancelled
        tokio::select! {
            result = pipeline.start() => {
                match result {
                    Ok(_) => {
                        info!("CDC pipeline started, waiting for completion");
                        // Wait for pipeline to complete
                        if let Err(e) = pipeline.wait().await {
                            error!("CDC pipeline error: {}", e);
                            return Err(e.into());
                        }
                    }
                    Err(e) => {
                        error!("CDC pipeline start error: {}", e);
                        return Err(e.into());
                    }
                }
            }
            _ = self.cancellation_token.cancelled() => {
                info!("CDC process cancelled, shutting down");
            }
        }

        Ok(())
    }
}
