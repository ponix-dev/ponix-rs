mod config;

use anyhow::Result;
use ponix_proto::envelope::v1::ProcessedEnvelope;
use ponix_runner::Runner;
use prost::Message;
use prost_types::Timestamp;
use std::time::{Duration, SystemTime};
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    // Initialize tracing
    let config = match config::ServiceConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&config.log_level)))
        .init();

    info!("Starting ponix-all-in-one service");
    info!("Configuration: {:?}", config);

    // Create runner with the main service process
    let runner = Runner::new()
        .with_app_process({
            let config = config.clone();
            move |ctx| {
                Box::pin(async move {
                    run_service(ctx, config).await
                })
            }
        })
        .with_closer(|| {
            Box::pin(async move {
                info!("Running cleanup tasks...");
                // Placeholder for future cleanup
                tokio::time::sleep(Duration::from_secs(1)).await;
                info!("Cleanup complete");
                Ok(())
            })
        })
        .with_closer_timeout(Duration::from_secs(10));

    // Run the service (this will handle the exit)
    runner.run().await;
}

async fn run_service(
    ctx: tokio_util::sync::CancellationToken,
    config: config::ServiceConfig,
) -> Result<()> {
    info!("Service started successfully");

    let interval = Duration::from_secs(config.interval_secs);

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping service");
                break;
            }
            _ = tokio::time::sleep(interval) => {
                // Generate unique, sortable ID using xid
                let id = xid::new();

                // Get current time for timestamps
                let now = SystemTime::now();
                let timestamp = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| Timestamp {
                        seconds: d.as_secs() as i64,
                        nanos: d.subsec_nanos() as i32,
                    })
                    .ok();

                // Create a ProcessedEnvelope message
                let envelope = ProcessedEnvelope {
                    end_device_id: id.to_string(),
                    occurred_at: timestamp,
                    data: None, // Could populate with sample data if needed
                    processed_at: timestamp,
                    organization_id: "example-org".to_string(),
                };

                // Log the envelope details
                info!(
                    end_device_id = %envelope.end_device_id,
                    occurred_at = ?envelope.occurred_at,
                    processed_at = ?envelope.processed_at,
                    organization_id = %envelope.organization_id,
                    "{}",
                    config.message
                );

                // Demonstrate serialization
                let encoded = envelope.encode_to_vec();
                tracing::debug!("Serialized envelope to {} bytes", encoded.len());
            }
        }
    }

    info!("Service stopped gracefully");
    Ok(())
}
