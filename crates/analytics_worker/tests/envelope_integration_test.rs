#![cfg(feature = "integration-tests")]

use analytics_worker::domain::CelPayloadConverter;
use analytics_worker::domain::RawEnvelopeService;
use common::domain::{DomainError, RawEnvelope};
use std::sync::Arc;

// Mock implementations for integration testing
mod mocks {
    use async_trait::async_trait;
    use common::domain::{
        CreateDeviceRepoInput, CreateOrganizationRepoInputWithId, DeleteOrganizationRepoInput,
        Device, DeviceRepository, DeviceWithDefinition, DomainResult, GetDeviceRepoInput,
        GetDeviceWithDefinitionRepoInput, GetOrganizationRepoInput, GetUserOrganizationsRepoInput,
        ListDevicesRepoInput, ListOrganizationsRepoInput, Organization, OrganizationRepository,
        ProcessedEnvelope, ProcessedEnvelopeProducer, UpdateOrganizationRepoInput,
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
                name: d.name.clone(),
                created_at: d.created_at,
                updated_at: d.updated_at,
            }))
        }

        async fn list_devices(&self, _input: ListDevicesRepoInput) -> DomainResult<Vec<Device>> {
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
        definition_id: "def-001".to_string(),
        definition_name: "Temperature Sensor Def".to_string(),
        name: "Temperature Sensor".to_string(),
        payload_conversion: "cayenne_lpp_decode(input)".to_string(),
        json_schema: "{}".to_string(),
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

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
    );

    // Cayenne LPP payload: channel 1, temperature 27.2°C
    let raw_envelope = RawEnvelope {
        organization_id: "org-123".to_string(),
        end_device_id: "sensor-001".to_string(),
        occurred_at: chrono::Utc::now(),
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
    assert_eq!(processed.occurred_at, raw_envelope.occurred_at);
    assert!(processed.data.contains_key("temperature_1"));
}

#[tokio::test]
async fn test_full_conversion_flow_custom_transformation() {
    // Arrange: Create device with definition containing custom CEL transformation
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-002".to_string(),
        organization_id: "org-456".to_string(),
        definition_id: "def-002".to_string(),
        definition_name: "Multi Sensor Def".to_string(),
        name: "Multi Sensor".to_string(),
        payload_conversion: r#"
            {
                'temp_c': cayenne_lpp_decode(input).temperature_1,
                'temp_f': cayenne_lpp_decode(input).temperature_1 * 9.0 / 5.0 + 32.0,
                'humidity': cayenne_lpp_decode(input).humidity_2,
                'status': 'active'
            }
        "#
        .to_string(),
        json_schema: "{}".to_string(),
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

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
    );

    // Cayenne LPP payload: temperature + humidity
    let raw_envelope = RawEnvelope {
        organization_id: "org-456".to_string(),
        end_device_id: "sensor-002".to_string(),
        occurred_at: chrono::Utc::now(),
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

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
    );

    let raw_envelope = RawEnvelope {
        organization_id: "org-999".to_string(),
        end_device_id: "nonexistent-device".to_string(),
        occurred_at: chrono::Utc::now(),
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
async fn test_invalid_cel_expression() {
    // Arrange: Device with definition containing invalid CEL expression
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-bad".to_string(),
        organization_id: "org-789".to_string(),
        definition_id: "def-bad".to_string(),
        definition_name: "Broken Definition".to_string(),
        name: "Broken Sensor".to_string(),
        payload_conversion: "invalid{[syntax".to_string(),
        json_schema: "{}".to_string(),
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

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
    );

    let raw_envelope = RawEnvelope {
        organization_id: "org-789".to_string(),
        end_device_id: "sensor-bad".to_string(),
        occurred_at: chrono::Utc::now(),
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
        definition_id: "def-empty".to_string(),
        definition_name: "Unconfigured Definition".to_string(),
        name: "Unconfigured Sensor".to_string(),
        payload_conversion: "".to_string(),
        json_schema: "{}".to_string(),
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

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
    );

    let raw_envelope = RawEnvelope {
        organization_id: "org-000".to_string(),
        end_device_id: "sensor-empty".to_string(),
        occurred_at: chrono::Utc::now(),
        payload: vec![0x01, 0x67, 0x01, 0x10],
    };

    // Act
    let result = service.process_raw_envelope(raw_envelope).await;

    // Assert
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::MissingCelExpression(_)
    ));
    assert_eq!(producer.get_published().len(), 0);
}

#[tokio::test]
async fn test_cel_expression_returns_non_object() {
    // Arrange: Device with definition containing CEL expression that returns a non-object
    let device = common::domain::DeviceWithDefinition {
        device_id: "sensor-scalar".to_string(),
        organization_id: "org-scalar".to_string(),
        definition_id: "def-scalar".to_string(),
        definition_name: "Scalar Definition".to_string(),
        name: "Scalar Sensor".to_string(),
        payload_conversion: "42".to_string(), // Returns number, not object
        json_schema: "{}".to_string(),
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

    let service = RawEnvelopeService::new(
        Arc::new(device_repo),
        Arc::new(org_repo),
        Arc::new(payload_converter),
        Arc::new(producer.clone()),
    );

    let raw_envelope = RawEnvelope {
        organization_id: "org-scalar".to_string(),
        end_device_id: "sensor-scalar".to_string(),
        occurred_at: chrono::Utc::now(),
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
