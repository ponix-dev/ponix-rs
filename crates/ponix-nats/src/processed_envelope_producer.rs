#[cfg(feature = "processed-envelope")]
use anyhow::{Context, Result};
#[cfg(feature = "processed-envelope")]
use crate::traits::JetStreamPublisher;
#[cfg(feature = "processed-envelope")]
use ponix_proto::envelope::v1::ProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use prost::Message;
#[cfg(feature = "processed-envelope")]
use std::sync::Arc;
#[cfg(feature = "processed-envelope")]
use tracing::{debug, info};

#[cfg(feature = "processed-envelope")]
pub struct ProcessedEnvelopeProducer {
    jetstream: Arc<dyn JetStreamPublisher>,
    base_subject: String,
}

#[cfg(feature = "processed-envelope")]
impl ProcessedEnvelopeProducer {
    pub fn new(jetstream: Arc<dyn JetStreamPublisher>, base_subject: String) -> Self {
        info!(
            "Created ProcessedEnvelopeProducer with base subject: {}",
            base_subject
        );
        Self {
            jetstream,
            base_subject,
        }
    }

    pub async fn publish(&self, envelope: &ProcessedEnvelope) -> Result<()> {
        // Serialize protobuf message
        let payload = envelope.encode_to_vec();

        // Build subject: {base_subject}.{end_device_id}
        let subject = format!("{}.{}", self.base_subject, envelope.end_device_id);

        debug!(
            subject = %subject,
            end_device_id = %envelope.end_device_id,
            size_bytes = payload.len(),
            "Publishing ProcessedEnvelope"
        );

        // Use the trait method
        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .context("Failed to publish and acknowledge message")?;

        info!(
            subject = %subject,
            end_device_id = %envelope.end_device_id,
            "Successfully published ProcessedEnvelope"
        );

        Ok(())
    }
}

#[cfg(all(test, feature = "processed-envelope"))]
mod tests {
    use super::*;
    use crate::traits::MockJetStreamPublisher;
    use mockall::predicate::*;

    // Helper to create a test ProcessedEnvelope
    fn create_test_envelope(end_device_id: &str) -> ProcessedEnvelope {
        ProcessedEnvelope {
            end_device_id: end_device_id.to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_producer_creation() {
        let mock_jetstream = MockJetStreamPublisher::new();
        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        assert_eq!(producer.base_subject, "processed.envelopes");
    }

    #[tokio::test]
    async fn test_publish_success() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Set up expectation for successful publish
        mock_jetstream
            .expect_publish()
            .withf(|subject: &String, payload: &bytes::Bytes| {
                subject.starts_with("processed.envelopes.") && !payload.is_empty()
            })
            .times(1)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope = create_test_envelope("device-123");
        let result = producer.publish(&envelope).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_with_correct_subject() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Verify the subject format is correct
        mock_jetstream
            .expect_publish()
            .with(
                eq("processed.envelopes.device-123".to_string()),
                always(),
            )
            .times(1)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope = create_test_envelope("device-123");
        let result = producer.publish(&envelope).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_serializes_correctly() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Capture the payload to verify serialization
        mock_jetstream
            .expect_publish()
            .times(1)
            .withf(|_subject: &String, payload: &bytes::Bytes| {
                // Verify payload can be deserialized back
                ProcessedEnvelope::decode(payload.as_ref()).is_ok()
            })
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope = create_test_envelope("device-123");
        let result = producer.publish(&envelope).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_failure() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Simulate publish failure
        mock_jetstream
            .expect_publish()
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("Failed to publish to JetStream")));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope = create_test_envelope("device-123");
        let result = producer.publish(&envelope).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to publish"));
    }

    #[tokio::test]
    async fn test_publish_multiple_envelopes() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Should publish each envelope separately
        mock_jetstream
            .expect_publish()
            .times(3)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        for i in 0..3 {
            let envelope = create_test_envelope(&format!("device-{}", i));
            let result = producer.publish(&envelope).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_publish_different_device_ids() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Verify subjects change based on end_device_id
        mock_jetstream
            .expect_publish()
            .with(eq("processed.envelopes.device-A".to_string()), always())
            .times(1)
            .returning(|_, _| Ok(()));

        mock_jetstream
            .expect_publish()
            .with(eq("processed.envelopes.device-B".to_string()), always())
            .times(1)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope_a = create_test_envelope("device-A");
        let envelope_b = create_test_envelope("device-B");

        assert!(producer.publish(&envelope_a).await.is_ok());
        assert!(producer.publish(&envelope_b).await.is_ok());
    }
}
