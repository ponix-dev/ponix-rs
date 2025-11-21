use ponix_domain::{
    EnvelopeService,
    RawEnvelope,
    DomainError,
};
use ponix_payload::CelPayloadConverter;
use std::sync::Arc;

// Mock implementations for integration testing
mod mocks {
    use ponix_domain::{
        Device,
        GetDeviceInput,
        ProcessedEnvelope,
        repository::{DeviceRepository, ProcessedEnvelopeProducer},
        error::DomainResult,
    };
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    pub struct InMemoryDeviceRepository {
        devices: Mutex<std::collections::HashMap<String, Device>>,
    }

    impl InMemoryDeviceRepository {
        pub fn new() -> Self {
            Self {
                devices: Mutex::new(std::collections::HashMap::new()),
            }
        }

        pub fn add_device(&self, device: Device) {
            let mut devices = self.devices.lock().unwrap();
            devices.insert(device.device_id.clone(), device);
        }
    }

    #[async_trait]
    impl DeviceRepository for InMemoryDeviceRepository {
        async fn create_device(
            &self,
            _input: ponix_domain::types::CreateDeviceInputWithId,
        ) -> DomainResult<Device> {
            unimplemented!("Not needed for envelope tests")
        }

        async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Option<Device>> {
            let devices = self.devices.lock().unwrap();
            Ok(devices.get(&input.device_id).cloned())
        }

        async fn list_devices(
            &self,
            _input: ponix_domain::types::ListDevicesInput,
        ) -> DomainResult<Vec<Device>> {
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
        async fn publish(&self, envelope: &ProcessedEnvelope) -> DomainResult<()> {
            let mut published = self.published.lock().unwrap();
            published.push(envelope.clone());
            Ok(())
        }
    }
}

#[tokio::test]
async fn test_full_conversion_flow_cayenne_lpp() {
    // Arrange: Create device with Cayenne LPP CEL expression
    let device = ponix_domain::Device {
        device_id: "sensor-001".to_string(),
        organization_id: "org-123".to_string(),
        name: "Temperature Sensor".to_string(),
        payload_conversion: "cayenne_lpp_decode(input)".to_string(),
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();

    let service = EnvelopeService::new(
        Arc::new(device_repo),
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
    // Arrange: Create device with custom CEL transformation
    let device = ponix_domain::Device {
        device_id: "sensor-002".to_string(),
        organization_id: "org-456".to_string(),
        name: "Multi Sensor".to_string(),
        payload_conversion: r#"
            {
                'temp_c': cayenne_lpp_decode(input).temperature_1,
                'temp_f': cayenne_lpp_decode(input).temperature_1 * 9.0 / 5.0 + 32.0,
                'humidity': cayenne_lpp_decode(input).humidity_2,
                'status': 'active'
            }
        "#.to_string(),
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();

    let service = EnvelopeService::new(
        Arc::new(device_repo),
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
            0x02, 0x68, 0x50,       // Channel 2: humidity 80%
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
    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();

    let service = EnvelopeService::new(
        Arc::new(device_repo),
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
    assert!(matches!(result.unwrap_err(), DomainError::DeviceNotFound(_)));
    assert_eq!(producer.get_published().len(), 0);
}

#[tokio::test]
async fn test_invalid_cel_expression() {
    // Arrange: Device with invalid CEL expression
    let device = ponix_domain::Device {
        device_id: "sensor-bad".to_string(),
        organization_id: "org-789".to_string(),
        name: "Broken Sensor".to_string(),
        payload_conversion: "invalid{[syntax".to_string(),
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();

    let service = EnvelopeService::new(
        Arc::new(device_repo),
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
    assert!(matches!(result.unwrap_err(), DomainError::PayloadConversionError(_)));
    assert_eq!(producer.get_published().len(), 0);
}

#[tokio::test]
async fn test_empty_cel_expression() {
    // Arrange: Device with empty CEL expression
    let device = ponix_domain::Device {
        device_id: "sensor-empty".to_string(),
        organization_id: "org-000".to_string(),
        name: "Unconfigured Sensor".to_string(),
        payload_conversion: "".to_string(),
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();

    let service = EnvelopeService::new(
        Arc::new(device_repo),
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
    assert!(matches!(result.unwrap_err(), DomainError::MissingCelExpression(_)));
    assert_eq!(producer.get_published().len(), 0);
}

#[tokio::test]
async fn test_cel_expression_returns_non_object() {
    // Arrange: Device with CEL expression that returns a non-object
    let device = ponix_domain::Device {
        device_id: "sensor-scalar".to_string(),
        organization_id: "org-scalar".to_string(),
        name: "Scalar Sensor".to_string(),
        payload_conversion: "42".to_string(), // Returns number, not object
        created_at: None,
        updated_at: None,
    };

    let device_repo = mocks::InMemoryDeviceRepository::new();
    device_repo.add_device(device);

    let payload_converter = CelPayloadConverter::new();
    let producer = mocks::InMemoryProducer::new();

    let service = EnvelopeService::new(
        Arc::new(device_repo),
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
    assert!(matches!(result.unwrap_err(), DomainError::PayloadConversionError(_)));
    assert_eq!(producer.get_published().len(), 0);
}
