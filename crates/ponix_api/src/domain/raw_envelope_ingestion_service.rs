use common::domain::{
    DataStreamRepository, DomainError, DomainResult, GetDataStreamWithDefinitionRepoInput,
    RawEnvelope, RawEnvelopeProducer,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Input for ingesting a raw envelope with data stream ownership validation
#[derive(Debug, Clone, Validate)]
pub struct CreateRawEnvelopeInput {
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub data_stream_id: String,
    #[garde(skip)]
    pub payload: Vec<u8>,
}

/// Domain service that validates data stream ownership before publishing raw envelopes
///
/// Flow:
/// 1. Validate input fields
/// 2. Verify data stream exists and belongs to the claimed organization
/// 3. Build RawEnvelope with current timestamp
/// 4. Publish via raw envelope producer
pub struct RawEnvelopeIngestionService {
    data_stream_repository: Arc<dyn DataStreamRepository>,
    raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
}

impl RawEnvelopeIngestionService {
    pub fn new(
        data_stream_repository: Arc<dyn DataStreamRepository>,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    ) -> Self {
        Self {
            data_stream_repository,
            raw_envelope_producer,
        }
    }

    /// Ingest a raw envelope after validating data stream ownership
    ///
    /// Returns `DataStreamNotFound` if the data stream doesn't exist or doesn't belong
    /// to the claimed organization (avoids leaking data stream existence across orgs).
    #[instrument(skip(self), fields(data_stream_id = %input.data_stream_id, organization_id = %input.organization_id))]
    pub async fn ingest(&self, input: CreateRawEnvelopeInput) -> DomainResult<()> {
        common::garde::validate_struct(&input)?;

        debug!(
            data_stream_id = %input.data_stream_id,
            org_id = %input.organization_id,
            payload_size = input.payload.len(),
            "validating data stream ownership before ingestion"
        );

        // Verify data stream exists and belongs to the claimed organization
        let _data_stream = self
            .data_stream_repository
            .get_data_stream_with_definition(GetDataStreamWithDefinitionRepoInput {
                data_stream_id: input.data_stream_id.clone(),
                organization_id: input.organization_id.clone(),
            })
            .await?
            .ok_or_else(|| DomainError::DataStreamNotFound(input.data_stream_id.clone()))?;

        let envelope = RawEnvelope {
            organization_id: input.organization_id,
            data_stream_id: input.data_stream_id,
            received_at: chrono::Utc::now(),
            payload: input.payload,
        };

        self.raw_envelope_producer
            .publish_raw_envelope(&envelope)
            .await?;

        debug!("successfully ingested raw envelope");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::{
        DataStreamWithDefinition, MockDataStreamRepository, MockRawEnvelopeProducer,
    };

    fn create_test_data_stream() -> DataStreamWithDefinition {
        use common::domain::PayloadContract;
        DataStreamWithDefinition {
            data_stream_id: "ds-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Data Stream".to_string(),
            contracts: vec![PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: "{}".to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            }],
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn test_ingest_success() {
        let mut mock_data_stream_repo = MockDataStreamRepository::new();
        let mut mock_producer = MockRawEnvelopeProducer::new();

        let data_stream = create_test_data_stream();

        mock_data_stream_repo
            .expect_get_data_stream_with_definition()
            .withf(|input: &GetDataStreamWithDefinitionRepoInput| {
                input.data_stream_id == "ds-123" && input.organization_id == "org-456"
            })
            .times(1)
            .return_once(move |_| Ok(Some(data_stream)));

        mock_producer
            .expect_publish_raw_envelope()
            .withf(|envelope: &RawEnvelope| {
                envelope.organization_id == "org-456"
                    && envelope.data_stream_id == "ds-123"
                    && envelope.payload == vec![0x01, 0x02, 0x03]
            })
            .times(1)
            .return_once(|_| Ok(()));

        let service = RawEnvelopeIngestionService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_producer),
        );

        let input = CreateRawEnvelopeInput {
            organization_id: "org-456".to_string(),
            data_stream_id: "ds-123".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ingest_data_stream_not_found() {
        let mut mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_producer = MockRawEnvelopeProducer::new();

        mock_data_stream_repo
            .expect_get_data_stream_with_definition()
            .times(1)
            .return_once(|_| Ok(None));

        let service = RawEnvelopeIngestionService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_producer),
        );

        let input = CreateRawEnvelopeInput {
            organization_id: "org-456".to_string(),
            data_stream_id: "ds-999".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(matches!(result, Err(DomainError::DataStreamNotFound(_))));
    }

    #[tokio::test]
    async fn test_ingest_publish_error() {
        let mut mock_data_stream_repo = MockDataStreamRepository::new();
        let mut mock_producer = MockRawEnvelopeProducer::new();

        let data_stream = create_test_data_stream();

        mock_data_stream_repo
            .expect_get_data_stream_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(data_stream)));

        mock_producer
            .expect_publish_raw_envelope()
            .times(1)
            .return_once(|_| {
                Err(DomainError::RepositoryError(anyhow::anyhow!(
                    "NATS publish failed"
                )))
            });

        let service = RawEnvelopeIngestionService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_producer),
        );

        let input = CreateRawEnvelopeInput {
            organization_id: "org-456".to_string(),
            data_stream_id: "ds-123".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(matches!(result, Err(DomainError::RepositoryError(_))));
    }

    #[tokio::test]
    async fn test_ingest_empty_organization_id_validation() {
        let mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_producer = MockRawEnvelopeProducer::new();

        let service = RawEnvelopeIngestionService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_producer),
        );

        let input = CreateRawEnvelopeInput {
            organization_id: "".to_string(),
            data_stream_id: "ds-123".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_ingest_empty_data_stream_id_validation() {
        let mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_producer = MockRawEnvelopeProducer::new();

        let service = RawEnvelopeIngestionService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_producer),
        );

        let input = CreateRawEnvelopeInput {
            organization_id: "org-456".to_string(),
            data_stream_id: "".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }
}
