#[cfg(feature = "processed-envelope")]
use crate::traits::JetStreamPublisher;
#[cfg(feature = "processed-envelope")]
use anyhow::Context;
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

}

#[cfg(all(test, feature = "processed-envelope"))]
mod tests {
    use super::*;
    use crate::traits::MockJetStreamPublisher;

    #[tokio::test]
    async fn test_producer_creation() {
        let mock_jetstream = MockJetStreamPublisher::new();
        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        assert_eq!(producer.base_subject, "processed.envelopes");
    }
}

// Domain trait implementation
#[cfg(feature = "processed-envelope")]
use ponix_domain::repository::ProcessedEnvelopeProducer as ProcessedEnvelopeProducerTrait;
#[cfg(feature = "processed-envelope")]
use ponix_domain::types::ProcessedEnvelope as DomainProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use ponix_domain::error::{DomainError, DomainResult};

#[cfg(feature = "processed-envelope")]
#[async_trait::async_trait]
impl ProcessedEnvelopeProducerTrait for ProcessedEnvelopeProducer {
    async fn publish(&self, envelope: &DomainProcessedEnvelope) -> DomainResult<()> {
        // Convert domain ProcessedEnvelope to protobuf
        let proto_envelope = crate::conversions::domain_to_proto_envelope(envelope);

        // Serialize protobuf message
        let payload = proto_envelope.encode_to_vec();

        // Build subject: {base_subject}.{end_device_id}
        let subject = format!("{}.{}", self.base_subject, proto_envelope.end_device_id);

        debug!(
            subject = %subject,
            end_device_id = %proto_envelope.end_device_id,
            size_bytes = payload.len(),
            "Publishing ProcessedEnvelope"
        );

        // Use the trait method
        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .context("Failed to publish and acknowledge message")
            .map_err(DomainError::RepositoryError)?;

        info!(
            subject = %subject,
            end_device_id = %proto_envelope.end_device_id,
            "Successfully published ProcessedEnvelope"
        );

        Ok(())
    }
}

#[cfg(all(test, feature = "processed-envelope"))]
mod domain_trait_tests {
    use super::*;
    use ponix_domain::repository::ProcessedEnvelopeProducer as ProcessedEnvelopeProducerTrait;
    use ponix_domain::types::ProcessedEnvelope as DomainProcessedEnvelope;
    use crate::traits::MockJetStreamPublisher;

    #[tokio::test]
    async fn test_domain_trait_publish_success() {
        // Arrange
        let mut mock_publisher = MockJetStreamPublisher::new();

        mock_publisher
            .expect_publish()
            .times(1)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_publisher),
            "test.stream".to_string(),
        );

        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));

        let envelope = DomainProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: chrono::Utc::now(),
            processed_at: chrono::Utc::now(),
            data,
        };

        // Act
        let result = ProcessedEnvelopeProducerTrait::publish(&producer, &envelope).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_domain_trait_publish_error() {
        // Arrange
        let mut mock_publisher = MockJetStreamPublisher::new();

        mock_publisher
            .expect_publish()
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("NATS publish failed")));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_publisher),
            "test.stream".to_string(),
        );

        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));

        let envelope = DomainProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: chrono::Utc::now(),
            processed_at: chrono::Utc::now(),
            data,
        };

        // Act
        let result = ProcessedEnvelopeProducerTrait::publish(&producer, &envelope).await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result, Err(ponix_domain::error::DomainError::RepositoryError(_))));
    }
}
