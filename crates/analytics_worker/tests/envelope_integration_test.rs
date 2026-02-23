#![cfg(feature = "integration-tests")]

use analytics_worker::domain::{CelPayloadConverter, RawEnvelopeService};
use common::cel::{CelCompiler, CelExpressionCompiler};
use common::domain::{DomainError, PayloadContract, RawEnvelope};
use common::jsonschema::JsonSchemaValidator;
use std::sync::Arc;

/// Helper: compile match + transform expressions into a PayloadContract with real compiled bytes
fn compiled_contract(match_expr: &str, transform_expr: &str, json_schema: &str) -> PayloadContract {
    let compiler = CelCompiler::new();
    PayloadContract {
        match_expression: match_expr.to_string(),
        transform_expression: transform_expr.to_string(),
        json_schema: json_schema.to_string(),
        compiled_match: compiler
            .compile(match_expr)
            .expect("match compilation failed"),
        compiled_transform: compiler
            .compile(transform_expr)
            .expect("transform compilation failed"),
    }
}

// Mock implementations for integration testing
mod mocks {
    use async_trait::async_trait;
    use common::domain::{
        CreateDeviceRepoInput, CreateOrganizationRepoInputWithId, DeleteOrganizationRepoInput,
        Device, DeviceRepository, DeviceWithDefinition, DomainResult, GetDeviceRepoInput,
        GetDeviceWithDefinitionRepoInput, GetOrganizationRepoInput, GetUserOrganizationsRepoInput,
        ListDevicesByGatewayRepoInput, ListDevicesRepoInput, ListOrganizationsRepoInput,
        Organization, OrganizationRepository, ProcessedEnvelope, ProcessedEnvelopeProducer,
        UpdateOrganizationRepoInput,
    };
    use std::sync::{Arc, Mutex};

    pub struct InMemoryDeviceRepository {
        devices: Mutex<std::collections::HashMap<String, DeviceWithDefinition>>,
    }

    impl InMemoryDeviceRepository {
        pub fn new() -> Self {
            Self {
                devices: Mutex::new(std::collections::HashMap::new()),
            }
        }

        pub fn add_device(&self, device: DeviceWithDefinition) {
            let mut devices = self.devices.lock().unwrap();
            devices.insert(device.device_id.clone(), device);
        }
    }

    #[async_trait]
    impl DeviceRepository for InMemoryDeviceRepository {
        async fn create_device(&self, _input: CreateDeviceRepoInput) -> DomainResult<Device> {
            unimplemented!("Not needed for envelope tests")
        }

        async fn get_device(&self, input: GetDeviceRepoInput) -> DomainResult<Option<Device>> {
            let devices = self.devices.lock().unwrap();
            Ok(devices.get(&input.device_id).map(|d| Device {
                device_id: d.device_id.clone(),
                organization_id: d.organization_id.clone(),
                definition_id: d.definition_id.clone(),
                workspace_id: d.workspace_id.clone(),
                gateway_id: d.gateway_id.clone(),
                name: d.name.clone(),
                created_at: d.created_at,
                updated_at: d.updated_at,
            }))
        }

        async fn list_devices(&self, _input: ListDevicesRepoInput) -> DomainResult<Vec<Device>> {
            unimplemented!("Not needed for envelope tests")
        }

        async fn list_devices_by_gateway(
            &self,
            _input: ListDevicesByGatewayRepoInput,
        ) -> DomainResult<Vec<Device>> {
            unimplemented!("Not needed for envelope tests")
        }

        async fn get_device_with_definition(
            &self,
            input: GetDeviceWithDefinitionRepoInput,
        ) -> DomainResult<Option<DeviceWithDefinition>> {
            let devices = self.devices.lock().unwrap();
            Ok(devices.get(&input.device_id).cloned())
        }
    }

    pub struct InMemoryOrganizationRepository {
        organizations: Mutex<std::collections::HashMap<String, Organization>>,
    }

    impl InMemoryOrganizationRepository {
        pub fn new() -> Self {
            Self {
                organizations: Mutex::new(std::collections::HashMap::new()),
            }
        }

        pub fn add_organization(&self, org: Organization) {
            let mut orgs = self.organizations.lock().unwrap();
            orgs.insert(org.id.clone(), org);
        }
    }

    #[async_trait]
    impl OrganizationRepository for InMemoryOrganizationRepository {
        async fn create_organization(
            &self,
            _input: CreateOrganizationRepoInputWithId,
        ) -> DomainResult<Organization> {
            unimplemented!("Not needed for envelope tests")
        }

        async fn get_organization(
            &self,
            input: GetOrganizationRepoInput,
        ) -> DomainResult<Option<Organization>> {
            let orgs = self.organizations.lock().unwrap();
            Ok(orgs.get(&input.organization_id).cloned())
        }

        async fn delete_organization(
            &self,
            _input: DeleteOrganizationRepoInput,
        ) -> DomainResult<()> {
            unimplemented!("Not needed for envelope tests")
        }

        async fn update_organization(
            &self,
            _input: UpdateOrganizationRepoInput,
        ) -> DomainResult<Organization> {
            unimplemented!("Not needed for envelope tests")
        }

        async fn list_organizations(
            &self,
            _input: ListOrganizationsRepoInput,
        ) -> DomainResult<Vec<Organization>> {
            unimplemented!("Not needed for envelope tests")
        }

        async fn get_organizations_by_user_id(
            &self,
            _input: GetUserOrganizationsRepoInput,
        ) -> DomainResult<Vec<Organization>> {
            unimplemented!("Not needed for envelope tests")
        }
    }

    #[derive(Clone)]
    pub struct InMemoryProducer {
        published: Arc<Mutex<Vec<ProcessedEnvelope>>>,
    }

    impl InMemoryProducer {
        pub fn new() -> Self {
            Self {
                published: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub fn get_published(&self) -> Vec<ProcessedEnvelope> {
            self.published.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ProcessedEnvelopeProducer for InMemoryProducer {
        async fn publish_processed_envelope(
            &self,
            envelope: &ProcessedEnvelope,
        ) -> DomainResult<()> {
            let mut published = self.published.lock().unwrap();
            published.push(envelope.clone());
            Ok(())
        }
    }
}

#[tokio::test]
async fn test_full_conversion_flow_cayenne_lpp() {
    // Arrange: Create device with definition containing Cayenne LPP CEL expression
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-001".to_string(),
        organization_id: "org-123".to_string(),
        workspace_id: "ws-123".to_string(),
        definition_id: "def-001".to_string(),
        gateway_id: "gw-001".to_string(),
        definition_name: "Temperature Sensor Def".to_string(),
        name: "Temperature Sensor".to_string(),
        contracts: vec![compiled_contract("true", "cayenne_lpp_decode(input)", "{}")],
        created_at: None,
        updated_at: None,
    };

    let org = common::domain::Organization {
        id: "org-123".to_string(),
        name: "Test Org".to_string(),
        deleted_at: None,
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let org_repo = mocks::InMemoryOrganizationRepository::new();
    org_repo.add_organization(org);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();
    let schema_validator = JsonSchemaValidator::new();

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
        Arc::new(schema_validator),
    );

    // Cayenne LPP payload: channel 1, temperature 27.2°C
    let raw_envelope = RawEnvelope {
        organization_id: "org-123".to_string(),
        end_device_id: "sensor-001".to_string(),
        received_at: chrono::Utc::now(),
        payload: vec![0x01, 0x67, 0x01, 0x10],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope.clone()).await;

    // Assert
    assert!(result.is_ok());

    let published = producer.get_published();
    assert_eq!(published.len(), 1);

    let processed = &published[0];
    assert_eq!(processed.organization_id, "org-123");
    assert_eq!(processed.end_device_id, "sensor-001");
    assert_eq!(processed.received_at, raw_envelope.received_at);
    assert!(processed.data.contains_key("temperature_1"));
}

#[tokio::test]
async fn test_full_conversion_flow_custom_transformation() {
    // Arrange: Create device with definition containing custom CEL transformation
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-002".to_string(),
        organization_id: "org-456".to_string(),
        workspace_id: "ws-456".to_string(),
        definition_id: "def-002".to_string(),
        gateway_id: "gw-001".to_string(),
        definition_name: "Multi Sensor Def".to_string(),
        name: "Multi Sensor".to_string(),
        contracts: vec![compiled_contract(
            "true",
            r#"
            {
                'temp_c': cayenne_lpp_decode(input).temperature_1,
                'temp_f': cayenne_lpp_decode(input).temperature_1 * 9.0 / 5.0 + 32.0,
                'humidity': cayenne_lpp_decode(input).humidity_2,
                'status': 'active'
            }
            "#,
            "{}",
        )],
        created_at: None,
        updated_at: None,
    };

    let org = common::domain::Organization {
        id: "org-456".to_string(),
        name: "Test Org".to_string(),
        deleted_at: None,
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let org_repo = mocks::InMemoryOrganizationRepository::new();
    org_repo.add_organization(org);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();
    let schema_validator = JsonSchemaValidator::new();

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
        Arc::new(schema_validator),
    );

    // Cayenne LPP payload: temperature + humidity
    let raw_envelope = RawEnvelope {
        organization_id: "org-456".to_string(),
        end_device_id: "sensor-002".to_string(),
        received_at: chrono::Utc::now(),
        payload: vec![
            0x01, 0x67, 0x01, 0x10, // Channel 1: temperature 27.2°C
            0x02, 0x68, 0x50, // Channel 2: humidity 80%
        ],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope).await;

    // Assert
    assert!(result.is_ok());

    let published = producer.get_published();
    assert_eq!(published.len(), 1);

    let processed = &published[0];
    assert!(processed.data.contains_key("temp_c"));
    assert!(processed.data.contains_key("temp_f"));
    assert!(processed.data.contains_key("humidity"));
    assert!(processed.data.contains_key("status"));
    assert_eq!(processed.data["status"], "active");
}

#[tokio::test]
async fn test_device_not_found() {
    // Arrange: Empty device repository
    let device_repo = mocks::InMemoryDeviceRepository::new();
    let org_repo = mocks::InMemoryOrganizationRepository::new();
    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();
    let schema_validator = JsonSchemaValidator::new();

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
        Arc::new(schema_validator),
    );

    let raw_envelope = RawEnvelope {
        organization_id: "org-999".to_string(),
        end_device_id: "nonexistent-device".to_string(),
        received_at: chrono::Utc::now(),
        payload: vec![0x01, 0x67, 0x01, 0x10],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope).await;

    // Assert
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DeviceNotFound(_)
    ));
    assert_eq!(producer.get_published().len(), 0);
}

#[tokio::test]
async fn test_invalid_compiled_bytes() {
    // Arrange: Device with definition containing corrupt compiled bytes
    // (simulates data corruption or schema migration issues)
    let compiler = CelCompiler::new();
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-bad".to_string(),
        organization_id: "org-789".to_string(),
        workspace_id: "ws-789".to_string(),
        definition_id: "def-bad".to_string(),
        gateway_id: "gw-001".to_string(),
        definition_name: "Broken Definition".to_string(),
        name: "Broken Sensor".to_string(),
        contracts: vec![PayloadContract {
            match_expression: "true".to_string(),
            transform_expression: "invalid{[syntax".to_string(),
            json_schema: "{}".to_string(),
            compiled_match: compiler.compile("true").unwrap(),
            compiled_transform: vec![0xFF, 0xFE, 0xFD], // corrupt bytes
        }],
        created_at: None,
        updated_at: None,
    };

    let org = common::domain::Organization {
        id: "org-789".to_string(),
        name: "Test Org".to_string(),
        deleted_at: None,
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let org_repo = mocks::InMemoryOrganizationRepository::new();
    org_repo.add_organization(org);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();
    let schema_validator = JsonSchemaValidator::new();

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
        Arc::new(schema_validator),
    );

    let raw_envelope = RawEnvelope {
        organization_id: "org-789".to_string(),
        end_device_id: "sensor-bad".to_string(),
        received_at: chrono::Utc::now(),
        payload: vec![0x01, 0x67, 0x01, 0x10],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope).await;

    // Assert
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::PayloadConversionError(_)
    ));
    assert_eq!(producer.get_published().len(), 0);
}

#[tokio::test]
async fn test_empty_cel_expression() {
    // Arrange: Device with definition containing empty CEL expression
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-empty".to_string(),
        organization_id: "org-000".to_string(),
        workspace_id: "ws-000".to_string(),
        definition_id: "def-empty".to_string(),
        gateway_id: "gw-001".to_string(),
        definition_name: "Unconfigured Definition".to_string(),
        name: "Unconfigured Sensor".to_string(),
        contracts: vec![], // Empty contracts - fails garde validation
        created_at: None,
        updated_at: None,
    };

    let org = common::domain::Organization {
        id: "org-000".to_string(),
        name: "Test Org".to_string(),
        deleted_at: None,
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let org_repo = mocks::InMemoryOrganizationRepository::new();
    org_repo.add_organization(org);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();
    let schema_validator = JsonSchemaValidator::new();

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
        Arc::new(schema_validator),
    );

    let raw_envelope = RawEnvelope {
        organization_id: "org-000".to_string(),
        end_device_id: "sensor-empty".to_string(),
        received_at: chrono::Utc::now(),
        payload: vec![0x01, 0x67, 0x01, 0x10],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope).await;

    // Assert - garde validates that contracts is non-empty
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::ValidationError(_)
    ));
    assert_eq!(producer.get_published().len(), 0);
}

#[tokio::test]
async fn test_cel_expression_returns_non_object() {
    // Arrange: Device with definition containing CEL expression that returns a non-object
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-scalar".to_string(),
        organization_id: "org-scalar".to_string(),
        workspace_id: "ws-scalar".to_string(),
        definition_id: "def-scalar".to_string(),
        gateway_id: "gw-001".to_string(),
        definition_name: "Scalar Definition".to_string(),
        name: "Scalar Sensor".to_string(),
        contracts: vec![compiled_contract("true", "42", "{}")],
        created_at: None,
        updated_at: None,
    };

    let org = common::domain::Organization {
        id: "org-scalar".to_string(),
        name: "Test Org".to_string(),
        deleted_at: None,
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let org_repo = mocks::InMemoryOrganizationRepository::new();
    org_repo.add_organization(org);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();
    let schema_validator = JsonSchemaValidator::new();

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
        Arc::new(schema_validator),
    );

    let raw_envelope = RawEnvelope {
        organization_id: "org-scalar".to_string(),
        end_device_id: "sensor-scalar".to_string(),
        received_at: chrono::Utc::now(),
        payload: vec![0x01, 0x67, 0x01, 0x10],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope).await;

    // Assert
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::PayloadConversionError(_)
    ));
    assert_eq!(producer.get_published().len(), 0);
}

#[tokio::test]
async fn test_multi_contract_routing_with_compiled_match() {
    // Arrange: Two contracts with different match expressions based on payload size.
    // Only the second contract should match a 4-byte payload.
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-multi".to_string(),
        organization_id: "org-multi".to_string(),
        workspace_id: "ws-multi".to_string(),
        definition_id: "def-multi".to_string(),
        gateway_id: "gw-001".to_string(),
        definition_name: "Multi Contract Def".to_string(),
        name: "Multi Contract Sensor".to_string(),
        contracts: vec![
            // Contract 0: only matches payloads > 10 bytes (won't match our 4-byte payload)
            compiled_contract("size(input) > 10", "{'source': 'large_payload'}", "{}"),
            // Contract 1: matches payloads <= 10 bytes (will match our 4-byte payload)
            compiled_contract("size(input) <= 10", "cayenne_lpp_decode(input)", "{}"),
        ],
        created_at: None,
        updated_at: None,
    };

    let org = common::domain::Organization {
        id: "org-multi".to_string(),
        name: "Test Org".to_string(),
        deleted_at: None,
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let org_repo = mocks::InMemoryOrganizationRepository::new();
    org_repo.add_organization(org);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();
    let schema_validator = JsonSchemaValidator::new();

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
        Arc::new(schema_validator),
    );

    // 4-byte Cayenne LPP payload: temperature 27.2°C
    let raw_envelope = RawEnvelope {
        organization_id: "org-multi".to_string(),
        end_device_id: "sensor-multi".to_string(),
        received_at: chrono::Utc::now(),
        payload: vec![0x01, 0x67, 0x01, 0x10],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope).await;

    // Assert: second contract matched, so we get cayenne_lpp_decode output (not 'large_payload')
    assert!(result.is_ok());

    let published = producer.get_published();
    assert_eq!(published.len(), 1);

    let processed = &published[0];
    assert!(
        processed.data.contains_key("temperature_1"),
        "Expected cayenne_lpp_decode output from second contract, got: {:?}",
        processed.data
    );
    assert!(
        !processed.data.contains_key("source"),
        "First contract should not have matched"
    );
}

#[tokio::test]
async fn test_compile_time_rejection_of_invalid_cel() {
    // Verify that invalid CEL expressions are caught at compile time (write path),
    // not at runtime (read path). This is the core value proposition of pre-compilation.
    let compiler = CelCompiler::new();

    let invalid_expressions = vec![
        "invalid{[syntax",
        "undefined_function(input)",
        "input + 'string'", // type error: bytes + string
    ];

    for expr in invalid_expressions {
        let result = compiler.compile(expr);
        assert!(
            result.is_err(),
            "Expected compile error for '{}', but got Ok",
            expr
        );
    }
}

#[tokio::test]
async fn test_compiled_bytes_roundtrip_through_service() {
    // Verify that compiled bytes survive the full path:
    // compile → store in PayloadContract → evaluate_match → transform → publish
    // This uses a non-trivial transform expression to ensure the compiled AST is meaningful.
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-roundtrip".to_string(),
        organization_id: "org-roundtrip".to_string(),
        workspace_id: "ws-roundtrip".to_string(),
        definition_id: "def-roundtrip".to_string(),
        gateway_id: "gw-001".to_string(),
        definition_name: "Roundtrip Def".to_string(),
        name: "Roundtrip Sensor".to_string(),
        contracts: vec![compiled_contract(
            "size(input) == 4",
            r#"
            {
                'temp_c': cayenne_lpp_decode(input).temperature_1,
                'temp_f': cayenne_lpp_decode(input).temperature_1 * 9.0 / 5.0 + 32.0
            }
            "#,
            "{}",
        )],
        created_at: None,
        updated_at: None,
    };

    let org = common::domain::Organization {
        id: "org-roundtrip".to_string(),
        name: "Test Org".to_string(),
        deleted_at: None,
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let org_repo = mocks::InMemoryOrganizationRepository::new();
    org_repo.add_organization(org);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();
    let schema_validator = JsonSchemaValidator::new();

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
        Arc::new(schema_validator),
    );

    // Cayenne LPP: temperature 27.2°C (exactly 4 bytes, matching our size(input) == 4)
    let raw_envelope = RawEnvelope {
        organization_id: "org-roundtrip".to_string(),
        end_device_id: "sensor-roundtrip".to_string(),
        received_at: chrono::Utc::now(),
        payload: vec![0x01, 0x67, 0x01, 0x10],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope).await;

    // Assert
    assert!(result.is_ok());

    let published = producer.get_published();
    assert_eq!(published.len(), 1);

    let processed = &published[0];
    assert_eq!(processed.data["temp_c"], 27.2);
    // 27.2 * 9/5 + 32 = 80.96
    let temp_f = processed.data["temp_f"].as_f64().unwrap();
    assert!(
        (temp_f - 80.96).abs() < 0.01,
        "Expected ~80.96, got {}",
        temp_f
    );
}
