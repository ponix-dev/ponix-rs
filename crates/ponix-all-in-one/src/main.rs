mod config;

use anyhow::Result;
use ponix_runner::Runner;
use std::time::Duration;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

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
    let mut counter = 0u64;

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping service");
                break;
            }
            _ = tokio::time::sleep(interval) => {
                counter += 1;
                info!("{} (iteration: {})", config.message, counter);
            }
        }
    }

    info!("Service stopped gracefully");
    Ok(())
}
