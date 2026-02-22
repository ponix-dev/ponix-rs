use crate::domain::PayloadConverter;
use common::domain::{
    DeviceRepository, DomainError, DomainResult, GetDeviceWithDefinitionRepoInput,
    GetOrganizationRepoInput, OrganizationRepository, ProcessedEnvelope, ProcessedEnvelopeProducer,
    RawEnvelope,
};
use common::garde::validate_struct;
use common::jsonschema::SchemaValidator;
use std::sync::Arc;
use tracing::{debug, error, instrument, warn};

/// Domain service that orchestrates raw → processed envelope conversion
///
/// Flow:
/// 1. Fetch device to get CEL expression and JSON schema
/// 2. Validate organization is not deleted
/// 3. Convert binary payload using CEL expression
/// 4. Validate converted JSON against device's JSON Schema
/// 5. Build ProcessedEnvelope from JSON + metadata
/// 6. Publish via producer trait
pub struct RawEnvelopeService {
    device_repository: Arc<dyn DeviceRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    payload_converter: Arc<dyn PayloadConverter>,
    producer: Arc<dyn ProcessedEnvelopeProducer>,
    schema_validator: Arc<dyn SchemaValidator>,
}

impl RawEnvelopeService {
    /// Create a new RawEnvelopeService with dependencies
    pub fn new(
        device_repository: Arc<dyn DeviceRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        payload_converter: Arc<dyn PayloadConverter>,
        producer: Arc<dyn ProcessedEnvelopeProducer>,
        schema_validator: Arc<dyn SchemaValidator>,
    ) -> Self {
        Self {
            device_repository,
            organization_repository,
            payload_converter,
            producer,
            schema_validator,
        }
    }

    /// Process a raw envelope: fetch device, validate org, convert payload, publish result
    #[instrument(skip(self), fields(device_id = %raw.end_device_id, organization_id = %raw.organization_id))]
    pub async fn process_raw_envelope(&self, raw: RawEnvelope) -> DomainResult<()> {
        debug!(
            device_id = %raw.end_device_id,
            org_id = %raw.organization_id,
            payload_size = raw.payload.len(),
            "processing raw envelope"
        );

        let device = self
            .device_repository
            .get_device_with_definition(GetDeviceWithDefinitionRepoInput {
                device_id: raw.end_device_id.clone(),
                organization_id: raw.organization_id.clone(),
            })
            .await?
            .ok_or_else(|| DomainError::DeviceNotFound(raw.end_device_id.clone()))?;

        // 2. Validate organization is not deleted
        debug!(organization_id = %device.organization_id, "validating organization status");
        match self
            .organization_repository
            .get_organization(GetOrganizationRepoInput {
                organization_id: device.organization_id.clone(),
            })
            .await?
        {
            Some(org) if org.deleted_at.is_some() => {
                warn!(
                    device_id = %raw.end_device_id,
                    org_id = %device.organization_id,
                    "rejecting envelope from deleted organization"
                );
                return Err(DomainError::OrganizationDeleted(format!(
                    "Cannot process envelope from deleted organization: {}",
                    device.organization_id
                )));
            }
            None => {
                warn!(
                    device_id = %raw.end_device_id,
                    org_id = %device.organization_id,
                    "rejecting envelope from non-existent organization"
                );
                return Err(DomainError::OrganizationNotFound(format!(
                    "Organization not found: {}",
                    device.organization_id
                )));
            }
            Some(_) => {
                // Organization exists and is active, continue processing
            }
        }

        // 3. Validate device with definition using garde
        validate_struct(&device)?;

        debug!(
            device_id = %raw.end_device_id,
            definition_id = %device.definition_id,
            expression = %device.payload_conversion,
            "converting payload with CEL expression"
        );

        // 4. Convert binary payload using CEL expression from definition
        let json_value = self
            .payload_converter
            .convert(&device.payload_conversion, &raw.payload)?;

        // 5. Validate converted JSON against device's JSON Schema (if schema exists)
        // TODO: Add metrics counter for schema validation attempts
        // TODO: Consider dead-letter queue for failed envelopes

        debug!(
            device_id = %raw.end_device_id,
            "validating converted payload against JSON Schema"
        );
        if let Err(validation_error) = self
            .schema_validator
            .validate(&device.json_schema, &json_value)
        {
            // Schema validation failures are expected errors - log and skip (ACK)
            // The data doesn't match the schema, retrying won't help
            warn!(
                device_id = %raw.end_device_id,
                reason = %validation_error.message,
                "envelope failed JSON Schema validation, skipping"
            );
            return Ok(()); // Return Ok so message gets ACK'd
        }

        // 6. Convert JSON Value to Map
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

        // 7. Build ProcessedEnvelope with current timestamp
        let processed_envelope = ProcessedEnvelope {
            organization_id: raw.organization_id.clone(),
            end_device_id: raw.end_device_id.clone(),
            received_at: raw.received_at,
            processed_at: chrono::Utc::now(),
            data,
        };

        debug!(
            device_id = %raw.end_device_id,
            field_count = processed_envelope.data.len(),
            "successfully converted payload"
        );

        // 8. Publish via producer trait
        self.producer
            .publish_processed_envelope(&processed_envelope)
            .await?;

        debug!(
            device_id = %raw.end_device_id,
            org_id = %raw.organization_id,
            "successfully processed and published envelope"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MockPayloadConverter;
    use common::domain::{
        DeviceWithDefinition, GetDeviceWithDefinitionRepoInput, MockDeviceRepository,
        MockOrganizationRepository, MockProcessedEnvelopeProducer, Organization,
    };
    use common::jsonschema::{MockSchemaValidator, SchemaValidationError};

    #[tokio::test]
    async fn test_process_raw_envelope_success() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mut mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = DeviceWithDefinition {
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
        };

        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10], // Cayenne LPP temperature
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .withf(|input: &GetDeviceWithDefinitionRepoInput| {
                input.device_id == "device-123" && input.organization_id == "org-456"
            })
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .withf(|input: &GetOrganizationRepoInput| input.organization_id == "org-456")
            .times(1)
            .return_once(move |_| Ok(Some(org)));

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

        mock_schema_validator
            .expect_validate()
            .times(1)
            .returning(|_, _| Ok(()));

        mock_producer
            .expect_publish_processed_envelope()
            .withf(|env: &ProcessedEnvelope| {
                env.end_device_id == "device-123"
                    && env.organization_id == "org-456"
                    && env.data.contains_key("temperature_1")
            })
            .times(1)
            .return_once(|_| Ok(()));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
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
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(|_| Ok(None));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-999".to_string(),
            received_at: chrono::Utc::now(),
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
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let device = DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "".to_string(), // Empty expression
            json_schema: "{}".to_string(),
            created_at: None,
            updated_at: None,
        };

        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert - garde validates that payload_conversion is non-empty
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_conversion_error() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let device = DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "invalid_expression".to_string(),
            json_schema: "{}".to_string(),
            created_at: None,
            updated_at: None,
        };

        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(org)));

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
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
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
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "42".to_string(), // Returns number, not object
            json_schema: "{}".to_string(),
            created_at: None,
            updated_at: None,
        };

        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        mock_converter
            .expect_convert()
            .times(1)
            .return_once(|_, _| Ok(serde_json::Value::Number(42.into())));

        // Schema validation is called before the object type check
        mock_schema_validator
            .expect_validate()
            .times(1)
            .returning(|_, _| Ok(()));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
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
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mut mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = DeviceWithDefinition {
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
        };

        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        let mut expected_json = serde_json::Map::new();
        expected_json.insert(
            "temperature_1".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(27.2).unwrap()),
        );

        mock_converter
            .expect_convert()
            .times(1)
            .return_once(move |_, _| Ok(serde_json::Value::Object(expected_json)));

        mock_schema_validator
            .expect_validate()
            .times(1)
            .returning(|_, _| Ok(()));

        mock_producer
            .expect_publish_processed_envelope()
            .times(1)
            .return_once(|_| {
                Err(DomainError::RepositoryError(anyhow::anyhow!(
                    "NATS publish failed"
                )))
            });

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(matches!(result, Err(DomainError::RepositoryError(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_organization_deleted() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let device = DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-deleted".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "cayenne_lpp_decode(input)".to_string(),
            json_schema: "{}".to_string(),
            created_at: None,
            updated_at: None,
        };

        let deleted_org = Organization {
            id: "org-deleted".to_string(),
            name: "Deleted Org".to_string(),
            deleted_at: Some(chrono::Utc::now()),
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .withf(|input: &GetOrganizationRepoInput| input.organization_id == "org-deleted")
            .times(1)
            .return_once(move |_| Ok(Some(deleted_org)));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-deleted".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(matches!(result, Err(DomainError::OrganizationDeleted(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_organization_not_found() {
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let device = DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-nonexistent".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "cayenne_lpp_decode(input)".to_string(),
            json_schema: "{}".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .withf(|input: &GetOrganizationRepoInput| input.organization_id == "org-nonexistent")
            .times(1)
            .return_once(|_| Ok(None));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-nonexistent".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert
        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_schema_validation_failed_returns_ok() {
        // Schema validation failures should return Ok() (message gets ACK'd, not retried)
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "cayenne_lpp_decode(input)".to_string(),
            json_schema: r#"{"type": "object", "required": ["temperature"]}"#.to_string(),
            created_at: None,
            updated_at: None,
        };

        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        // CEL conversion succeeds but returns data missing required field
        let mut json_data = serde_json::Map::new();
        json_data.insert("humidity".to_string(), serde_json::Value::Number(60.into()));

        mock_converter
            .expect_convert()
            .times(1)
            .return_once(move |_, _| Ok(serde_json::Value::Object(json_data)));

        // Schema validation fails
        mock_schema_validator
            .expect_validate()
            .times(1)
            .return_once(|_, _| {
                Err(SchemaValidationError {
                    message: "Missing required field: temperature".to_string(),
                })
            });

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
            payload: vec![0x01, 0x68, 0x3C], // Some payload
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert - should return Ok because schema validation failures are logged and skipped
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_raw_envelope_empty_schema_returns_validation_error() {
        // Empty schema "" fails garde validation and should return Err
        // Arrange
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_converter = MockPayloadConverter::new(); // Should not be called
        let mock_producer = MockProcessedEnvelopeProducer::new(); // Should not be called
        let mock_schema_validator = MockSchemaValidator::new(); // Should not be called

        let device = DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "cayenne_lpp_decode(input)".to_string(),
            json_schema: "".to_string(), // Empty schema - fails garde validation
            created_at: None,
            updated_at: None,
        };

        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let raw_envelope = RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        };

        // Act
        let result = service.process_raw_envelope(raw_envelope).await;

        // Assert - garde validates that json_schema is non-empty
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }
}
