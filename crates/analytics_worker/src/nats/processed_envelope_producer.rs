use common::nats::{
    JetStreamPublisher, LayeredPublisher, NatsPublishService, NatsPublisherBuilder,
    NatsTracingConfig, PublishRequest,
};

use prost::Message;
use std::sync::Arc;
use tower::Service;
use tracing::debug;

pub struct ProcessedEnvelopeProducer {
    publisher: LayeredPublisher<NatsPublishService>,
    base_subject: String,
}

impl ProcessedEnvelopeProducer {
    pub fn new(jetstream: Arc<dyn JetStreamPublisher>, base_subject: String) -> Self {
        debug!(
            stream = "processed_envelopes",
            base_subject = %base_subject,
            "initialized ProcessedEnvelopeProducer"
        );

        let publisher = NatsPublisherBuilder::new(jetstream)
            .with_tracing(NatsTracingConfig::new("processed_envelope_producer"))
            .with_logging()
            .build();

        Self {
            publisher,
            base_subject,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::nats::MockJetStreamPublisher;

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

use common::domain::{
    DomainError, DomainResult, ProcessedEnvelope as DomainProcessedEnvelope,
    ProcessedEnvelopeProducer as ProcessedEnvelopeProducerTrait,
};

#[async_trait::async_trait]
impl ProcessedEnvelopeProducerTrait for ProcessedEnvelopeProducer {
    async fn publish_processed_envelope(
        &self,
        envelope: &DomainProcessedEnvelope,
    ) -> DomainResult<()> {
        // Convert domain ProcessedEnvelope to protobuf
        let proto_envelope = common::proto::domain_to_proto_envelope(envelope);

        // Serialize protobuf message
        let payload = proto_envelope.encode_to_vec();

        // Build subject: {base_subject}.{data_stream_id}
        let subject = format!("{}.{}", self.base_subject, proto_envelope.data_stream_id);

        // Create request and call the middleware stack
        let request = PublishRequest::new(subject, payload);

        // Clone the publisher to call the service (Tower services are Clone)
        self.publisher
            .clone()
            .call(request)
            .await
            .map_err(DomainError::RepositoryError)?;

        Ok(())
    }
}

#[cfg(test)]
mod domain_trait_tests {
    use super::*;
    use common::domain::{
        ProcessedEnvelope as DomainProcessedEnvelope,
        ProcessedEnvelopeProducer as ProcessedEnvelopeProducerTrait,
    };
    use common::nats::MockJetStreamPublisher;

    #[tokio::test]
    async fn test_domain_trait_publish_success() {
        // Arrange
        let mut mock_publisher = MockJetStreamPublisher::new();

        // Middleware uses publish_with_headers instead of publish
        mock_publisher
            .expect_publish_with_headers()
            .times(1)
            .returning(|_, _, _| Ok(()));

        let producer =
            ProcessedEnvelopeProducer::new(Arc::new(mock_publisher), "test.stream".to_string());

        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));

        let envelope = DomainProcessedEnvelope {
            organization_id: "org-123".to_string(),
            data_stream_id: "ds-456".to_string(),
            received_at: chrono::Utc::now(),
            processed_at: chrono::Utc::now(),
            data,
        };

        // Act
        let result =
            ProcessedEnvelopeProducerTrait::publish_processed_envelope(&producer, &envelope).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_domain_trait_publish_error() {
        // Arrange
        let mut mock_publisher = MockJetStreamPublisher::new();

        // Middleware uses publish_with_headers instead of publish
        mock_publisher
            .expect_publish_with_headers()
            .times(1)
            .returning(|_, _, _| Err(anyhow::anyhow!("NATS publish failed")));

        let producer =
            ProcessedEnvelopeProducer::new(Arc::new(mock_publisher), "test.stream".to_string());

        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));

        let envelope = DomainProcessedEnvelope {
            organization_id: "org-123".to_string(),
            data_stream_id: "ds-456".to_string(),
            received_at: chrono::Utc::now(),
            processed_at: chrono::Utc::now(),
            data,
        };

        // Act
        let result =
            ProcessedEnvelopeProducerTrait::publish_processed_envelope(&producer, &envelope).await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(common::domain::DomainError::RepositoryError(_))
        ));
    }
}
