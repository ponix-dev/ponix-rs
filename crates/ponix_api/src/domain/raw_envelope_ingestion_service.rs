use common::domain::{
    DeviceRepository, DomainError, DomainResult, GetDeviceWithDefinitionRepoInput, RawEnvelope,
    RawEnvelopeProducer,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Input for ingesting a raw envelope with device ownership validation
#[derive(Debug, Clone, Validate)]
pub struct CreateRawEnvelopeInput {
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub end_device_id: String,
    #[garde(skip)]
    pub payload: Vec<u8>,
}

/// Domain service that validates device ownership before publishing raw envelopes
///
/// Flow:
/// 1. Validate input fields
/// 2. Verify device exists and belongs to the claimed organization
/// 3. Build RawEnvelope with current timestamp
/// 4. Publish via raw envelope producer
pub struct RawEnvelopeIngestionService {
    device_repository: Arc<dyn DeviceRepository>,
    raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
}

impl RawEnvelopeIngestionService {
    pub fn new(
        device_repository: Arc<dyn DeviceRepository>,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    ) -> Self {
        Self {
            device_repository,
            raw_envelope_producer,
        }
    }

    /// Ingest a raw envelope after validating device ownership
    ///
    /// Returns `DeviceNotFound` if the device doesn't exist or doesn't belong
    /// to the claimed organization (avoids leaking device existence across orgs).
    #[instrument(skip(self), fields(device_id = %input.end_device_id, organization_id = %input.organization_id))]
    pub async fn ingest(&self, input: CreateRawEnvelopeInput) -> DomainResult<()> {
        common::garde::validate_struct(&input)?;

        debug!(
            device_id = %input.end_device_id,
            org_id = %input.organization_id,
            payload_size = input.payload.len(),
            "validating device ownership before ingestion"
        );

        // Verify device exists and belongs to the claimed organization
        let _device = self
            .device_repository
            .get_device_with_definition(GetDeviceWithDefinitionRepoInput {
                device_id: input.end_device_id.clone(),
                organization_id: input.organization_id.clone(),
            })
            .await?
            .ok_or_else(|| DomainError::DeviceNotFound(input.end_device_id.clone()))?;

        let envelope = RawEnvelope {
            organization_id: input.organization_id,
            end_device_id: input.end_device_id,
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
    use common::domain::{DeviceWithDefinition, MockDeviceRepository, MockRawEnvelopeProducer};

    fn create_test_device() -> DeviceWithDefinition {
        DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "cayenne_lpp_decode(input)".to_string(),
            json_schema: "{}".to_string(),
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn test_ingest_success() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_producer = MockRawEnvelopeProducer::new();

        let device = create_test_device();

        mock_device_repo
            .expect_get_device_with_definition()
            .withf(|input: &GetDeviceWithDefinitionRepoInput| {
                input.device_id == "device-123" && input.organization_id == "org-456"
            })
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_producer
            .expect_publish_raw_envelope()
            .withf(|envelope: &RawEnvelope| {
                envelope.organization_id == "org-456"
                    && envelope.end_device_id == "device-123"
                    && envelope.payload == vec![0x01, 0x02, 0x03]
            })
            .times(1)
            .return_once(|_| Ok(()));

        let service =
            RawEnvelopeIngestionService::new(Arc::new(mock_device_repo), Arc::new(mock_producer));

        let input = CreateRawEnvelopeInput {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ingest_device_not_found() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_producer = MockRawEnvelopeProducer::new();

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(|_| Ok(None));

        let service =
            RawEnvelopeIngestionService::new(Arc::new(mock_device_repo), Arc::new(mock_producer));

        let input = CreateRawEnvelopeInput {
            organization_id: "org-456".to_string(),
            end_device_id: "device-999".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(matches!(result, Err(DomainError::DeviceNotFound(_))));
    }

    #[tokio::test]
    async fn test_ingest_publish_error() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_producer = MockRawEnvelopeProducer::new();

        let device = create_test_device();

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_producer
            .expect_publish_raw_envelope()
            .times(1)
            .return_once(|_| {
                Err(DomainError::RepositoryError(anyhow::anyhow!(
                    "NATS publish failed"
                )))
            });

        let service =
            RawEnvelopeIngestionService::new(Arc::new(mock_device_repo), Arc::new(mock_producer));

        let input = CreateRawEnvelopeInput {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(matches!(result, Err(DomainError::RepositoryError(_))));
    }

    #[tokio::test]
    async fn test_ingest_empty_organization_id_validation() {
        let mock_device_repo = MockDeviceRepository::new();
        let mock_producer = MockRawEnvelopeProducer::new();

        let service =
            RawEnvelopeIngestionService::new(Arc::new(mock_device_repo), Arc::new(mock_producer));

        let input = CreateRawEnvelopeInput {
            organization_id: "".to_string(),
            end_device_id: "device-123".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_ingest_empty_device_id_validation() {
        let mock_device_repo = MockDeviceRepository::new();
        let mock_producer = MockRawEnvelopeProducer::new();

        let service =
            RawEnvelopeIngestionService::new(Arc::new(mock_device_repo), Arc::new(mock_producer));

        let input = CreateRawEnvelopeInput {
            organization_id: "org-456".to_string(),
            end_device_id: "".to_string(),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = service.ingest(input).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }
}
