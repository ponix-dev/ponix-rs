use crate::error::{DomainError, DomainResult};
use crate::payload_converter::PayloadConverter;
use crate::repository::{DeviceRepository, ProcessedEnvelopeProducer};
use crate::types::{GetDeviceInput, ProcessedEnvelope, RawEnvelope};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Domain service that orchestrates raw → processed envelope conversion
///
/// Flow:
/// 1. Fetch device to get CEL expression
/// 2. Convert binary payload using CEL expression
/// 3. Build ProcessedEnvelope from JSON + metadata
/// 4. Publish via producer trait
pub struct RawEnvelopeService {
    device_repository: Arc<dyn DeviceRepository>,
    payload_converter: Arc<dyn PayloadConverter>,
    producer: Arc<dyn ProcessedEnvelopeProducer>,
}

impl RawEnvelopeService {
    /// Create a new RawEnvelopeService with dependencies
    pub fn new(
        device_repository: Arc<dyn DeviceRepository>,
        payload_converter: Arc<dyn PayloadConverter>,
        producer: Arc<dyn ProcessedEnvelopeProducer>,
    ) -> Self {
        Self {
            device_repository,
            payload_converter,
            producer,
        }
    }

    /// Process a raw envelope: fetch device, convert payload, publish result
    pub async fn process_raw_envelope(&self, raw: RawEnvelope) -> DomainResult<()> {
        debug!(
            device_id = %raw.end_device_id,
            org_id = %raw.organization_id,
            payload_size = raw.payload.len(),
            "Processing raw envelope"
        );

        // 1. Fetch device to get CEL expression
        let device = self
            .device_repository
            .get_device(GetDeviceInput {
                device_id: raw.end_device_id.clone(),
            })
            .await?
            .ok_or_else(|| DomainError::DeviceNotFound(raw.end_device_id.clone()))?;

        // 2. Validate CEL expression exists
        if device.payload_conversion.is_empty() {
            error!(
                device_id = %raw.end_device_id,
                "Device has empty CEL expression"
            );
            return Err(DomainError::MissingCelExpression(raw.end_device_id.clone()));
        }

        debug!(
            device_id = %raw.end_device_id,
            expression = %device.payload_conversion,
            "Converting payload with CEL expression"
        );

        // 3. Convert binary payload using CEL expression
        let json_value = self
            .payload_converter
            .convert(&device.payload_conversion, &raw.payload)?;

        // 4. Convert JSON Value to Map
        let data = match json_value {
            serde_json::Value::Object(map) => map,
            _ => {
                error!(
                    device_id = %raw.end_device_id,
                    "CEL expression did not return a JSON object"
                );
                return Err(DomainError::PayloadConversionError(
                    "CEL expression must return a JSON object".to_string(),
                ));
            }
        };

        // 5. Build ProcessedEnvelope with current timestamp
        let processed_envelope = ProcessedEnvelope {
            organization_id: raw.organization_id.clone(),
            end_device_id: raw.end_device_id.clone(),
            occurred_at: raw.occurred_at,
            processed_at: chrono::Utc::now(),
            data,
        };

        debug!(
            device_id = %raw.end_device_id,
            field_count = processed_envelope.data.len(),
            "Successfully converted payload"
        );

        // 6. Publish via producer trait
        self.producer.publish(&processed_envelope).await?;

        info!(
            device_id = %raw.end_device_id,
            org_id = %raw.organization_id,
            "Successfully processed and published envelope"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload_converter::MockPayloadConverter;
    use crate::repository::MockDeviceRepository;
    use crate::repository::MockProcessedEnvelopeProducer;
    use crate::types::Device;

    #[tokio::test]
    async fn test_process_raw_envelope_success() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mut mock_producer = MockProcessedEnvelopeProducer::new();

        let device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "cayenne_lpp_decode(input)".to_string(),
            created_at: None,
            updated_at: None,
        };

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10], // Cayenne LPP temperature
        };

        mock_device_repo
            .expect_get_device()
            .withf(|input: &GetDeviceInput| input.device_id == "device-123")
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        let mut expected_json = serde_json::Map::new();
        expected_json.insert(
            "temperature_1".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(27.2).unwrap()),
        );

        mock_converter
            .expect_convert()
            .withf(|expr: &str, _payload: &[u8]| expr == "cayenne_lpp_decode(input)")
            .times(1)
            .return_once(move |_, _| Ok(serde_json::Value::Object(expected_json)));

        mock_producer
            .expect_publish()
            .withf(|env: &ProcessedEnvelope| {
                env.end_device_id == "device-123"
                    && env.organization_id == "org-456"
                    && env.data.contains_key("temperature_1")
            })
            .times(1)
            .return_once(|_| Ok(()));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
        );

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_raw_envelope_device_not_found() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();

        mock_device_repo
            .expect_get_device()
            .times(1)
            .return_once(|_| Ok(None));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-999".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(matches!(result, Err(DomainError::DeviceNotFound(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_missing_cel_expression() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();

        let device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "".to_string(), // Empty expression
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(matches!(result, Err(DomainError::MissingCelExpression(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_conversion_error() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();

        let device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "invalid_expression".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_converter
            .expect_convert()
            .times(1)
            .return_once(|_, _| {
                Err(DomainError::PayloadConversionError(
                    "Invalid CEL expression".to_string(),
                ))
            });

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(matches!(
            result,
            Err(DomainError::PayloadConversionError(_))
        ));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_non_object_json() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();

        let device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "42".to_string(), // Returns number, not object
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_converter
            .expect_convert()
            .times(1)
            .return_once(|_, _| Ok(serde_json::Value::Number(42.into())));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(matches!(
            result,
            Err(DomainError::PayloadConversionError(_))
        ));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_publish_error() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mut mock_producer = MockProcessedEnvelopeProducer::new();

        let device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "cayenne_lpp_decode(input)".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        let mut expected_json = serde_json::Map::new();
        expected_json.insert(
            "temperature_1".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(27.2).unwrap()),
        );

        mock_converter
            .expect_convert()
            .times(1)
            .return_once(move |_, _| Ok(serde_json::Value::Object(expected_json)));

        mock_producer.expect_publish().times(1).return_once(|_| {
            Err(DomainError::RepositoryError(anyhow::anyhow!(
                "NATS publish failed"
            )))
        });

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(matches!(result, Err(DomainError::RepositoryError(_))));
    }
}
