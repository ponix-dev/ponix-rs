use anyhow::Result;
use ponix_domain::repository::ProcessedEnvelopeProducer as ProcessedEnvelopeProducerTrait;
use ponix_domain::ProcessedEnvelope;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

/// Configuration for the demo producer service
pub struct DemoProducerConfig {
    /// Interval between publishing messages
    pub interval: Duration,
    /// Organization ID to use for demo messages
    pub organization_id: String,
    /// Log message to display when publishing
    pub log_message: String,
}

impl Default for DemoProducerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            organization_id: "example-org".to_string(),
            log_message: "Published demo envelope".to_string(),
        }
    }
}

/// Run a demo producer service that publishes sample ProcessedEnvelope messages
///
/// This function continuously publishes sample envelopes at the specified interval
/// until a cancellation signal is received. It's useful for testing and demonstration
/// purposes.
///
/// # Arguments
///
/// * `ctx` - Cancellation token to stop the producer
/// * `config` - Configuration for the demo producer
/// * `producer` - Producer implementation that publishes envelopes
///
/// # Example
///
/// ```no_run
/// use ponix_nats::{run_demo_producer, DemoProducerConfig, ProcessedEnvelopeProducer};
/// use tokio_util::sync::CancellationToken;
/// use std::sync::Arc;
///
/// # async fn example(producer: Arc<ProcessedEnvelopeProducer>) {
/// let ctx = CancellationToken::new();
/// let config = DemoProducerConfig::default();
///
/// run_demo_producer(ctx, config, producer).await.unwrap();
/// # }
/// ```
pub async fn run_demo_producer<P>(
    ctx: CancellationToken,
    config: DemoProducerConfig,
    producer: P,
) -> Result<()>
where
    P: ProcessedEnvelopeProducerTrait,
{
    info!("Demo producer service started");

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping demo producer");
                break;
            }
            _ = tokio::time::sleep(config.interval) => {
                // Generate unique ID using xid
                let id = xid::new();

                // Create sample data for the envelope
                let mut data = serde_json::Map::new();
                data.insert("temperature".to_string(), serde_json::json!(72.5));
                data.insert("humidity".to_string(), serde_json::json!(65.0));
                data.insert("status".to_string(), serde_json::json!("active"));

                // Create ProcessedEnvelope
                let envelope = ProcessedEnvelope {
                    end_device_id: id.to_string(),
                    occurred_at: chrono::Utc::now(),
                    data,
                    processed_at: chrono::Utc::now(),
                    organization_id: config.organization_id.clone(),
                };

                // Publish to NATS JetStream
                match producer.publish(&envelope).await {
                    Ok(_) => {
                        debug!(
                            end_device_id = %envelope.end_device_id,
                            organization_id = %envelope.organization_id,
                            "{}",
                            config.log_message
                        );
                    }
                    Err(e) => {
                        error!(
                            end_device_id = %envelope.end_device_id,
                            error = %e,
                            "Failed to publish envelope"
                        );
                    }
                }
            }
        }
    }

    info!("Demo producer service stopped gracefully");
    Ok(())
}
