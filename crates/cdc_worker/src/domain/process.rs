use crate::domain::{CdcConfig, EntityConfig};
use crate::nats::NatsSink;

use anyhow::Result;
use common::nats::NatsClient;
use etl::config::{BatchConfig, PgConnectionConfig, PipelineConfig, TlsConfig};
use etl::pipeline::Pipeline;
use etl::store::both::memory::MemoryStore;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

pub struct CdcProcess {
    config: CdcConfig,
    nats_client: Arc<NatsClient>,
    entity_configs: Vec<EntityConfig>,
    cancellation_token: CancellationToken,
}

impl CdcProcess {
    pub fn new(
        config: CdcConfig,
        nats_client: Arc<NatsClient>,
        entity_configs: Vec<EntityConfig>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            config,
            nats_client,
            entity_configs,
            cancellation_token,
        }
    }

    pub async fn run(self) -> Result<()> {
        debug!(
            entity_count = self.entity_configs.len(),
            "starting CDC process"
        );

        // Create NATS publisher
        let publisher = self.nats_client.create_publisher_client();

        // Create NATS sink with provided entity configurations
        let destination = NatsSink::new(publisher, self.entity_configs);

        // Create in-memory store for state/schema management
        // TODO: we should probably understand what this does more when scaling to multiple instances
        let store = MemoryStore::new();

        // Configure PostgreSQL connection
        let pg_config = PgConnectionConfig {
            host: self.config.pg_host.clone(),
            port: self.config.pg_port,
            name: self.config.pg_database.clone(),
            username: self.config.pg_user.clone(),
            password: Some(self.config.pg_password.clone().into()),
            // TODO: support TLS configuration
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
                        info!("cdc pipeline started, waiting for completion");
                        // Wait for pipeline to complete
                        if let Err(e) = pipeline.wait().await {
                            error!(error = %e, "cdc pipeline error");
                            return Err(e.into());
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "cdc pipeline start error");
                        return Err(e.into());
                    }
                }
            }
            _ = self.cancellation_token.cancelled() => {
                debug!("cdc process cancelled, shutting down");
            }
        }

        Ok(())
    }
}
