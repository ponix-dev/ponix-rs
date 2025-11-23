use crate::traits::JetStreamPublisher;
use anyhow::{Context, Result};
use bytes::Bytes;
use chrono::Utc;
use ponix_proto::envelope::v1::RawEnvelope;
use prost::Message as ProstMessage;
use prost_types::Timestamp;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::info;

/// Configuration for the RawEnvelope demo producer
#[cfg(feature = "raw-envelope")]
#[derive(Debug, Clone)]
pub struct RawEnvelopeDemoProducerConfig {
    pub base_subject: String,
    pub interval_ms: u64,
    pub organization_id: String,
    pub device_id: String,
}

/// Run a demo producer that publishes sample RawEnvelope messages at regular intervals
#[cfg(feature = "raw-envelope")]
pub async fn run_raw_envelope_demo_producer(
    jetstream: Arc<dyn JetStreamPublisher>,
    config: RawEnvelopeDemoProducerConfig,
) -> Result<()> {
    info!(
        base_subject = %config.base_subject,
        interval_ms = config.interval_ms,
        organization_id = config.organization_id,
        device_id = config.device_id,
        "Starting RawEnvelope demo producer"
    );

    let mut ticker = interval(Duration::from_millis(config.interval_ms));
    let mut counter: u32 = 0;

    loop {
        ticker.tick().await;

        let now = Utc::now();

        // Generate sample binary payload (simulating sensor data)
        // This is a simple example - in reality this would be device-specific binary format
        let payload = vec![
            0x01,                          // Sensor type
            0x02,                          // Data length
            (counter & 0xFF) as u8,        // Low byte of counter
            ((counter >> 8) & 0xFF) as u8, // High byte of counter
        ];

        let envelope = RawEnvelope {
            organization_id: config.organization_id.clone(),
            device_id: config.device_id.clone(),
            received_at: Some(Timestamp {
                seconds: now.timestamp(),
                nanos: now.timestamp_subsec_nanos() as i32,
            }),
            payload: Bytes::from(payload),
        };

        let encoded = envelope.encode_to_vec();
        let size_bytes = encoded.len();
        let subject = format!("{}.{}", config.base_subject, config.device_id);

        jetstream
            .publish(subject.clone(), encoded.into())
            .await
            .context("Failed to publish RawEnvelope")?;

        info!(
            subject = %subject,
            device_id = %config.device_id,
            counter = counter,
            size_bytes = size_bytes,
            "Published sample RawEnvelope"
        );

        counter += 1;
    }
}

#[cfg(all(test, feature = "raw-envelope"))]
mod tests {
    use super::*;
    use crate::traits::MockJetStreamPublisher;

    #[tokio::test]
    async fn test_demo_producer_publishes_messages() {
        // Arrange
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Expect at least one publish call
        mock_jetstream
            .expect_publish()
            .withf(|subject: &String, _payload: &Bytes| {
                subject.starts_with("raw_envelopes.device-")
            })
            .times(1..)
            .returning(|_, _| Ok(()));

        let config = RawEnvelopeDemoProducerConfig {
            base_subject: "raw_envelopes".to_string(),
            interval_ms: 100,
            organization_id: "org-123".to_string(),
            device_id: "device-456".to_string(),
        };

        // Act - run for a short duration then cancel
        let jetstream = Arc::new(mock_jetstream);
        let producer_handle =
            tokio::spawn(async move { run_raw_envelope_demo_producer(jetstream, config).await });

        // Let it run for a bit
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Cancel the task
        producer_handle.abort();

        // The mock expectations will be verified on drop
    }
}
