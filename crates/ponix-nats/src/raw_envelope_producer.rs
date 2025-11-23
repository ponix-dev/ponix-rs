use crate::traits::JetStreamPublisher;
use anyhow::Context;
use async_trait::async_trait;
use ponix_domain::error::{DomainError, DomainResult};
use ponix_domain::RawEnvelope as DomainRawEnvelope;
use ponix_domain::RawEnvelopeProducer as RawEnvelopeProducerTrait;
use prost::Message as ProstMessage;
use std::sync::Arc;
use tracing::{debug, info};

/// NATS JetStream producer for RawEnvelope messages
#[cfg(feature = "raw-envelope")]
pub struct RawEnvelopeProducer {
    jetstream: Arc<dyn JetStreamPublisher>,
    base_subject: String,
}

#[cfg(feature = "raw-envelope")]
impl RawEnvelopeProducer {
    pub fn new(jetstream: Arc<dyn JetStreamPublisher>, base_subject: String) -> Self {
        info!(
            "Created RawEnvelopeProducer with base subject: {}",
            base_subject
        );
        Self {
            jetstream,
            base_subject,
        }
    }
}

#[cfg(feature = "raw-envelope")]
#[async_trait]
impl RawEnvelopeProducerTrait for RawEnvelopeProducer {
    async fn publish(&self, envelope: &DomainRawEnvelope) -> DomainResult<()> {
        // Convert domain RawEnvelope to protobuf
        let proto_envelope = crate::raw_envelope_conversions::domain_to_proto(envelope);

        // Serialize protobuf message
        let payload = proto_envelope.encode_to_vec();

        // Build subject: {base_subject}.{device_id}
        let subject = format!("{}.{}", self.base_subject, proto_envelope.device_id);

        debug!(
            subject = %subject,
            device_id = %proto_envelope.device_id,
            size_bytes = payload.len(),
            "Publishing RawEnvelope"
        );

        // Publish via JetStream
        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .context("Failed to publish and acknowledge message")
            .map_err(DomainError::RepositoryError)?;

        info!(
            subject = %subject,
            device_id = %proto_envelope.device_id,
            "Successfully published RawEnvelope"
        );

        Ok(())
    }
}

#[cfg(all(test, feature = "raw-envelope"))]
mod tests {
    use super::*;
    use crate::traits::MockJetStreamPublisher;
    use bytes::Bytes;

    #[tokio::test]
    async fn test_publish_success() {
        // Arrange
        let mut mock_jetstream = MockJetStreamPublisher::new();

        mock_jetstream
            .expect_publish()
            .withf(|subject: &String, _payload: &Bytes| {
                subject.starts_with("raw_envelopes.device-")
            })
            .times(1)
            .returning(|_, _| Ok(()));

        let producer =
            RawEnvelopeProducer::new(Arc::new(mock_jetstream), "raw_envelopes".to_string());

        let envelope = DomainRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x02, 0x03],
        };

        // Act
        let result = producer.publish(&envelope).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_failure() {
        // Arrange
        let mut mock_jetstream = MockJetStreamPublisher::new();

        mock_jetstream
            .expect_publish()
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("NATS publish failed")));

        let producer =
            RawEnvelopeProducer::new(Arc::new(mock_jetstream), "raw_envelopes".to_string());

        let envelope = DomainRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x02, 0x03],
        };

        // Act
        let result = producer.publish(&envelope).await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result, Err(DomainError::RepositoryError(_))));
    }
}
