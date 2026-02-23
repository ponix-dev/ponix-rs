use crate::domain::PayloadConverter;
use common::domain::{
    DeviceRepository, DomainError, DomainResult, GetDeviceWithDefinitionRepoInput,
    GetOrganizationRepoInput, OrganizationRepository, PayloadContract, ProcessedEnvelope,
    ProcessedEnvelopeProducer, RawEnvelope,
};
use common::garde::validate_struct;
use common::jsonschema::SchemaValidator;
use std::sync::Arc;
use tracing::{debug, error, instrument, warn};

/// Domain service that orchestrates raw → processed envelope conversion
///
/// Flow:
/// 1. Fetch device to get payload contracts
/// 2. Validate organization is not deleted
/// 3. Iterate contracts: match → transform → validate schema
/// 4. Build ProcessedEnvelope from JSON + metadata
/// 5. Publish via producer trait
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

    /// Process a raw envelope: fetch device, validate org, run contracts, publish result
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

        // 4. Process contracts: match → transform → validate
        let json_value =
            match self.process_contracts(&device.contracts, &raw.payload, &raw.end_device_id)? {
                Some(value) => value,
                None => {
                    // No contract matched or schema validation failed — ACK without publish
                    return Ok(());
                }
            };

        // 5. Convert JSON Value to Map
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

        // 6. Build ProcessedEnvelope with current timestamp
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

        // 7. Publish via producer trait
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

    /// Iterate through contracts in order: evaluate match → transform → validate schema
    ///
    /// Returns:
    /// - `Ok(Some(value))` if a contract matched, transformed, and validated successfully
    /// - `Ok(None)` if no contract matched, or a matched contract failed schema validation
    /// - `Err(...)` if match evaluation or transform execution failed with an error
    fn process_contracts(
        &self,
        contracts: &[PayloadContract],
        payload: &[u8],
        device_id: &str,
    ) -> DomainResult<Option<serde_json::Value>> {
        for (idx, contract) in contracts.iter().enumerate() {
            debug!(
                device_id = %device_id,
                contract_index = idx,
                match_expression = %contract.match_expression,
                "evaluating contract match expression"
            );

            let matched = self.payload_converter.evaluate_match(
                &contract.compiled_match,
                &contract.match_expression,
                payload,
            )?;

            if !matched {
                debug!(
                    device_id = %device_id,
                    contract_index = idx,
                    "contract match expression returned false, trying next"
                );
                continue;
            }

            debug!(
                device_id = %device_id,
                contract_index = idx,
                transform_expression = %contract.transform_expression,
                "contract matched, transforming payload"
            );

            let json_value = self.payload_converter.transform(
                &contract.compiled_transform,
                &contract.transform_expression,
                payload,
            )?;

            // Validate against contract's JSON Schema
            debug!(
                device_id = %device_id,
                contract_index = idx,
                "validating transformed payload against JSON Schema"
            );
            // A matching contract claims ownership of the payload. If schema validation
            // fails, the message is dropped (Ok(None)) rather than trying the next contract.
            if let Err(validation_error) = self
                .schema_validator
                .validate(&contract.json_schema, &json_value)
            {
                warn!(
                    device_id = %device_id,
                    contract_index = idx,
                    reason = %validation_error.message,
                    "envelope failed JSON Schema validation, skipping"
                );
                return Ok(None);
            }

            return Ok(Some(json_value));
        }

        warn!(
            device_id = %device_id,
            contract_count = contracts.len(),
            "no contract matched the payload, skipping"
        );
        Ok(None)
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

    fn make_device_with_contracts(contracts: Vec<PayloadContract>) -> DeviceWithDefinition {
        DeviceWithDefinition {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-001".to_string(),
            definition_name: "Test Definition".to_string(),
            name: "Test Device".to_string(),
            contracts,
            created_at: None,
            updated_at: None,
        }
    }

    fn default_contracts() -> Vec<PayloadContract> {
        vec![PayloadContract {
            match_expression: "true".to_string(),
            transform_expression: "cayenne_lpp_decode(input)".to_string(),
            json_schema: "{}".to_string(),
            compiled_match: vec![],
            compiled_transform: vec![],
        }]
    }

    fn active_org() -> Organization {
        Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn raw_envelope() -> RawEnvelope {
        RawEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-123".to_string(),
            received_at: chrono::Utc::now(),
            payload: vec![0x01, 0x67, 0x01, 0x10],
        }
    }

    #[tokio::test]
    async fn test_process_raw_envelope_success() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mut mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = make_device_with_contracts(default_contracts());

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
            .return_once(move |_| Ok(Some(active_org())));

        let mut expected_json = serde_json::Map::new();
        expected_json.insert(
            "temperature_1".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(27.2).unwrap()),
        );

        mock_converter
            .expect_evaluate_match()
            .withf(|_compiled: &[u8], source: &str, _payload: &[u8]| source == "true")
            .times(1)
            .return_once(|_, _, _| Ok(true));

        mock_converter
            .expect_transform()
            .withf(|_compiled: &[u8], source: &str, _payload: &[u8]| {
                source == "cayenne_lpp_decode(input)"
            })
            .times(1)
            .return_once(move |_, _, _| Ok(serde_json::Value::Object(expected_json)));

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

        let result = service.process_raw_envelope(raw_envelope()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_raw_envelope_device_not_found() {
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

        let result = service
            .process_raw_envelope(RawEnvelope {
                organization_id: "org-456".to_string(),
                end_device_id: "device-999".to_string(),
                received_at: chrono::Utc::now(),
                payload: vec![0x01, 0x67, 0x01, 0x10],
            })
            .await;

        assert!(matches!(result, Err(DomainError::DeviceNotFound(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_empty_contracts_returns_validation_error() {
        // Empty contracts vec fails garde validation (length min = 1)
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let device = make_device_with_contracts(vec![]); // Empty contracts

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(active_org())));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let result = service.process_raw_envelope(raw_envelope()).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_conversion_error() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let device = make_device_with_contracts(vec![PayloadContract {
            match_expression: "true".to_string(),
            transform_expression: "invalid_expression".to_string(),
            json_schema: "{}".to_string(),
            compiled_match: vec![],
            compiled_transform: vec![],
        }]);

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(active_org())));

        mock_converter
            .expect_evaluate_match()
            .times(1)
            .return_once(|_, _, _| Ok(true));

        mock_converter
            .expect_transform()
            .times(1)
            .return_once(|_, _, _| {
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

        let result = service.process_raw_envelope(raw_envelope()).await;
        assert!(matches!(
            result,
            Err(DomainError::PayloadConversionError(_))
        ));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_non_object_json() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = make_device_with_contracts(vec![PayloadContract {
            match_expression: "true".to_string(),
            transform_expression: "42".to_string(),
            json_schema: "{}".to_string(),
            compiled_match: vec![],
            compiled_transform: vec![],
        }]);

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(active_org())));

        mock_converter
            .expect_evaluate_match()
            .times(1)
            .return_once(|_, _, _| Ok(true));

        mock_converter
            .expect_transform()
            .times(1)
            .return_once(|_, _, _| Ok(serde_json::Value::Number(42.into())));

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

        let result = service.process_raw_envelope(raw_envelope()).await;
        assert!(matches!(
            result,
            Err(DomainError::PayloadConversionError(_))
        ));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_publish_error() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mut mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = make_device_with_contracts(default_contracts());

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(active_org())));

        let mut expected_json = serde_json::Map::new();
        expected_json.insert(
            "temperature_1".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(27.2).unwrap()),
        );

        mock_converter
            .expect_evaluate_match()
            .times(1)
            .return_once(|_, _, _| Ok(true));

        mock_converter
            .expect_transform()
            .times(1)
            .return_once(move |_, _, _| Ok(serde_json::Value::Object(expected_json)));

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

        let result = service.process_raw_envelope(raw_envelope()).await;
        assert!(matches!(result, Err(DomainError::RepositoryError(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_organization_deleted() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let mut device = make_device_with_contracts(default_contracts());
        device.organization_id = "org-deleted".to_string();

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .withf(|input: &GetOrganizationRepoInput| input.organization_id == "org-deleted")
            .times(1)
            .return_once(move |_| {
                Ok(Some(Organization {
                    id: "org-deleted".to_string(),
                    name: "Deleted Org".to_string(),
                    deleted_at: Some(chrono::Utc::now()),
                    created_at: None,
                    updated_at: None,
                }))
            });

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let result = service
            .process_raw_envelope(RawEnvelope {
                organization_id: "org-deleted".to_string(),
                end_device_id: "device-123".to_string(),
                received_at: chrono::Utc::now(),
                payload: vec![0x01, 0x67, 0x01, 0x10],
            })
            .await;

        assert!(matches!(result, Err(DomainError::OrganizationDeleted(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_organization_not_found() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let mut device = make_device_with_contracts(default_contracts());
        device.organization_id = "org-nonexistent".to_string();

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

        let result = service
            .process_raw_envelope(RawEnvelope {
                organization_id: "org-nonexistent".to_string(),
                end_device_id: "device-123".to_string(),
                received_at: chrono::Utc::now(),
                payload: vec![0x01, 0x67, 0x01, 0x10],
            })
            .await;

        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_process_raw_envelope_schema_validation_failed_returns_ok() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = make_device_with_contracts(vec![PayloadContract {
            match_expression: "true".to_string(),
            transform_expression: "cayenne_lpp_decode(input)".to_string(),
            json_schema: r#"{"type": "object", "required": ["temperature"]}"#.to_string(),
            compiled_match: vec![],
            compiled_transform: vec![],
        }]);

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(active_org())));

        let mut json_data = serde_json::Map::new();
        json_data.insert("humidity".to_string(), serde_json::Value::Number(60.into()));

        mock_converter
            .expect_evaluate_match()
            .times(1)
            .return_once(|_, _, _| Ok(true));

        mock_converter
            .expect_transform()
            .times(1)
            .return_once(move |_, _, _| Ok(serde_json::Value::Object(json_data)));

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

        let result = service.process_raw_envelope(raw_envelope()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_raw_envelope_no_contract_matches_returns_ok() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mock_producer = MockProcessedEnvelopeProducer::new();
        let mock_schema_validator = MockSchemaValidator::new();

        let device = make_device_with_contracts(vec![PayloadContract {
            match_expression: "false".to_string(),
            transform_expression: "cayenne_lpp_decode(input)".to_string(),
            json_schema: "{}".to_string(),
            compiled_match: vec![],
            compiled_transform: vec![],
        }]);

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(active_org())));

        mock_converter
            .expect_evaluate_match()
            .withf(|_compiled: &[u8], source: &str, _payload: &[u8]| source == "false")
            .times(1)
            .return_once(|_, _, _| Ok(false));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let result = service.process_raw_envelope(raw_envelope()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_raw_envelope_second_contract_matches() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_converter = MockPayloadConverter::new();
        let mut mock_producer = MockProcessedEnvelopeProducer::new();
        let mut mock_schema_validator = MockSchemaValidator::new();

        let device = make_device_with_contracts(vec![
            PayloadContract {
                match_expression: "false".to_string(),
                transform_expression: "should_not_run".to_string(),
                json_schema: "{}".to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            },
            PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: "{}".to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            },
        ]);

        mock_device_repo
            .expect_get_device_with_definition()
            .times(1)
            .return_once(move |_| Ok(Some(device)));

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(active_org())));

        // First contract: match returns false
        mock_converter
            .expect_evaluate_match()
            .withf(|_compiled: &[u8], source: &str, _payload: &[u8]| source == "false")
            .times(1)
            .return_once(|_, _, _| Ok(false));

        // Second contract: match returns true
        mock_converter
            .expect_evaluate_match()
            .withf(|_compiled: &[u8], source: &str, _payload: &[u8]| source == "true")
            .times(1)
            .return_once(|_, _, _| Ok(true));

        let mut expected_json = serde_json::Map::new();
        expected_json.insert(
            "temperature_1".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(27.2).unwrap()),
        );

        mock_converter
            .expect_transform()
            .withf(|_compiled: &[u8], source: &str, _payload: &[u8]| {
                source == "cayenne_lpp_decode(input)"
            })
            .times(1)
            .return_once(move |_, _, _| Ok(serde_json::Value::Object(expected_json)));

        mock_schema_validator
            .expect_validate()
            .times(1)
            .returning(|_, _| Ok(()));

        mock_producer
            .expect_publish_processed_envelope()
            .times(1)
            .return_once(|_| Ok(()));

        let service = RawEnvelopeService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_converter),
            Arc::new(mock_producer),
            Arc::new(mock_schema_validator),
        );

        let result = service.process_raw_envelope(raw_envelope()).await;
        assert!(result.is_ok());
    }
}
