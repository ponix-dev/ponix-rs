//! Basic example of using the ponix-runner
//!
//! This example demonstrates:
//! - Running multiple concurrent processes
//! - Graceful shutdown on SIGTERM/SIGINT (Ctrl+C)
//! - Cleanup with closers
//!
//! Run with: cargo run --example basic_runner

use ponix_runner::Runner;
use std::time::Duration;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting runner example");

    let runner = Runner::new()
        // First process: counter that increments every second
        .with_app_process(|ctx| async move {
            let mut counter = 0;
            loop {
                tokio::select! {
                    _ = ctx.cancelled() => {
                        tracing::info!("Counter process stopping gracefully at count: {}", counter);
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        counter += 1;
                        tracing::info!("Counter: {}", counter);
                    }
                }
            }
            Ok(())
        })
        // Second process: prints a message every 2 seconds
        .with_app_process(|ctx| async move {
            loop {
                tokio::select! {
                    _ = ctx.cancelled() => {
                        tracing::info!("Logger process stopping gracefully");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {
                        tracing::info!("Logger: Still running...");
                    }
                }
            }
            Ok(())
        })
        // Third process: simulates an error after 30 seconds (if not cancelled first)
        .with_app_process(|ctx| async move {
            tokio::select! {
                _ = ctx.cancelled() => {
                    tracing::info!("Error simulator stopping gracefully");
                    Ok(())
                }
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    tracing::error!("Simulated error occurred!");
                    Err(anyhow::anyhow!("Simulated error after 30 seconds"))
                }
            }
        })
        // Add cleanup closers
        .with_closer(|| async move {
            tracing::info!("Closer 1: Cleaning up resources...");
            tokio::time::sleep(Duration::from_millis(500)).await;
            tracing::info!("Closer 1: Done");
            Ok(())
        })
        .with_closer(|| async move {
            tracing::info!("Closer 2: Flushing buffers...");
            tokio::time::sleep(Duration::from_millis(300)).await;
            tracing::info!("Closer 2: Done");
            Ok(())
        })
        .with_closer_timeout(Duration::from_secs(5));

    tracing::info!("Press Ctrl+C to trigger graceful shutdown");
    runner.run().await;
}
