# Raw Envelope to Processed Envelope Conversion Service Implementation Plan

## Overview

Implement domain logic for converting raw envelopes (binary payloads) into processed envelopes (JSON data) using CEL (Common Expression Language) expressions. This feature enables configurable, per-device payload decoding where each device stores its own CEL expression for transforming binary sensor data into structured JSON.

## Current State Analysis

### What Already Exists

1. **CEL Infrastructure** ([ponix-payload/src/cel.rs:134-197](crates/ponix-payload/src/cel.rs#L134-L197))
   - `CelEnvironment::new()` - Creates environment with `cayenne_lpp_decode` function
   - `execute(expression: &str, input: &[u8]) -> Result<JsonValue>` - Executes CEL expressions
   - Comprehensive tests covering basic operations and Cayenne LPP decoding

2. **Domain Types**
   - `Device` entity with `payload_conversion: String` field ([ponix-domain/src/types.rs:8](crates/ponix-domain/src/types.rs#L8))
   - `ProcessedEnvelope` entity ([ponix-domain/src/types.rs:46-54](crates/ponix-domain/src/types.rs#L46-L54))
   - `DeviceRepository` trait with `get_device()` method ([ponix-domain/src/repository.rs:7-18](crates/ponix-domain/src/repository.rs#L7-L18))

3. **NATS Producer** ([ponix-nats/src/processed_envelope_producer.rs](crates/ponix-nats/src/processed_envelope_producer.rs))
   - Existing `ProcessedEnvelopeProducer` that publishes to NATS
   - Uses `JetStreamPublisher` trait internally
   - Needs refactoring to implement domain trait

### What's Missing

1. **RawEnvelope** domain type
2. **PayloadConverter** trait (domain layer abstraction)
3. **ProcessedEnvelopeProducer** trait (domain layer abstraction)
4. **EnvelopeService** (orchestration service)
5. **CelPayloadConverter** implementation
6. **Refactored NATS producer** implementing domain trait
7. **New domain error variants** for conversion failures

### Key Discoveries

- **Two-Input Pattern**: Domain services receive inputs without generated IDs, then create internal inputs with IDs ([ponix-domain/src/end_device_service.rs:19-48](crates/ponix-domain/src/end_device_service.rs#L19-L48))
- **Arc<dyn Trait> Pattern**: Services take trait objects wrapped in Arc for dependency injection ([ponix-domain/src/end_device_service.rs:10-17](crates/ponix-domain/src/end_device_service.rs#L10-L17))
- **Mockall Testing**: All traits use `#[cfg_attr(test, mockall::automock)]` for test mocking ([ponix-domain/src/repository.rs:7](crates/ponix-domain/src/repository.rs#L7))
- **Validation in Services**: Input validation happens in domain services, not repositories ([ponix-domain/src/end_device_service.rs:23-29](crates/ponix-domain/src/end_device_service.rs#L23-L29))

## Desired End State

### Functional Requirements
- Accept a `RawEnvelope` with binary payload and device ID
- Fetch the device's CEL expression from `DeviceRepository`
- Convert binary payload to JSON using `PayloadConverter` trait
- Build a `ProcessedEnvelope` with converted data and timestamps
- Publish the processed envelope via `ProcessedEnvelopeProducer` trait
- Return errors for missing devices, invalid CEL expressions, or conversion failures

### Verification Criteria
- Unit tests pass with mocked dependencies
- Integration tests verify end-to-end conversion with real CEL expressions
- Service can be instantiated with real and mock dependencies
- Error handling covers all failure scenarios
- Follows existing DDD patterns established in `DeviceService`

## What We're NOT Doing

- ✗ Raw envelope ingestion/reception mechanism (NATS consumer)
- ✗ gRPC endpoints for triggering envelope processing
- ✗ Batch processing of multiple raw envelopes (individual message processing only)
- ✗ CEL expression validation UI/endpoints
- ✗ Metrics and observability instrumentation
- ✗ Dead-letter queue for failed conversions
- ✗ Retry logic or circuit breakers

## Implementation Approach

This implementation follows the established domain-driven design pattern with three layers:
1. **Domain Layer** (`ponix-domain`) - Business logic, traits, and types
2. **Payload Layer** (`ponix-payload`) - CEL-based conversion implementation
3. **Infrastructure Layer** (`ponix-nats`) - NATS publishing implementation

The service will process individual messages (not batches) to support per-message NAK in NATS consumers.

---

## Phase 1: Domain Types and Errors

### Overview
Add the `RawEnvelope` domain type and new error variants to support conversion failures.

### Changes Required

#### 1. Add RawEnvelope Type
**File**: `crates/ponix-domain/src/types.rs`
**Location**: After `ProcessedEnvelope` definition (after line 60)

```rust
/// Raw envelope with binary payload before processing
#[derive(Debug, Clone, PartialEq)]
pub struct RawEnvelope {
    pub organization_id: String,
    pub end_device_id: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub payload: Vec<u8>,
}
```

#### 2. Export RawEnvelope
**File**: `crates/ponix-domain/src/lib.rs`
**Change**: Update line 11 to include `RawEnvelope` in exports

```rust
pub use types::{CreateDeviceInput, CreateDeviceInputWithId, Device, GetDeviceInput, ListDevicesInput, ProcessedEnvelope, RawEnvelope, StoreEnvelopesInput};
```

#### 3. Add Domain Error Variants
**File**: `crates/ponix-domain/src/error.rs`
**Location**: Add after line 20 (before `RepositoryError`)

```rust
#[error("Payload conversion error: {0}")]
PayloadConversionError(String),

#[error("Missing CEL expression for device: {0}")]
MissingCelExpression(String),
```

### Success Criteria

#### Automated Verification:
- [x] Code compiles: `cargo build -p ponix-domain`
- [x] Type checking passes: `cargo check -p ponix-domain`
- [x] No new clippy warnings: `cargo clippy -p ponix-domain`

#### Manual Verification:
- [x] `RawEnvelope` is accessible from other crates importing `ponix-domain`
- [x] Error variants display meaningful messages

---

## Phase 2: Domain Traits

### Overview
Define the `PayloadConverter` and `ProcessedEnvelopeProducer` traits in the domain layer.

### Changes Required

#### 1. Create PayloadConverter Trait
**File**: `crates/ponix-domain/src/payload_converter.rs` (new file)

```rust
use crate::error::DomainResult;

/// Trait for converting binary payloads to JSON using CEL expressions
///
/// Implementations should:
/// - Execute the provided CEL expression against the binary payload
/// - Return JSON value on success
/// - Return PayloadConversionError on failure
#[cfg_attr(test, mockall::automock)]
pub trait PayloadConverter: Send + Sync {
    /// Convert binary payload to JSON using a CEL expression
    ///
    /// # Arguments
    /// * `expression` - CEL expression string (e.g., "cayenne_lpp_decode(input)")
    /// * `payload` - Binary payload to convert
    ///
    /// # Returns
    /// JSON value as serde_json::Value on success
    fn convert(
        &self,
        expression: &str,
        payload: &[u8],
    ) -> DomainResult<serde_json::Value>;
}
```

#### 2. Add ProcessedEnvelopeProducer Trait
**File**: `crates/ponix-domain/src/repository.rs`
**Location**: After `ProcessedEnvelopeRepository` (after line 29)

```rust
/// Trait for publishing processed envelopes to message broker
///
/// Implementations should:
/// - Serialize envelope to appropriate format (protobuf)
/// - Publish to message broker (NATS JetStream)
/// - Return error if publish fails
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ProcessedEnvelopeProducer: Send + Sync {
    /// Publish a single processed envelope
    ///
    /// # Arguments
    /// * `envelope` - ProcessedEnvelope to publish
    ///
    /// # Returns
    /// () on success, DomainError on failure
    async fn publish(&self, envelope: &ProcessedEnvelope) -> DomainResult<()>;
}
```

#### 3. Update Module Exports
**File**: `crates/ponix-domain/src/lib.rs`
**Changes**:

Add module declaration (after line 5):
```rust
pub mod payload_converter;
```

Update exports (line 9):
```rust
pub use payload_converter::PayloadConverter;
```

Update exports (line 10):
```rust
pub use repository::{DeviceRepository, ProcessedEnvelopeProducer, ProcessedEnvelopeRepository};
```

### Success Criteria

#### Automated Verification:
- [x] Code compiles: `cargo build -p ponix-domain`
- [x] Mock generation works: `cargo test -p ponix-domain --lib`
- [x] No new clippy warnings: `cargo clippy -p ponix-domain`

#### Manual Verification:
- [x] Traits follow the same pattern as `DeviceRepository`
- [x] `#[cfg_attr(test, mockall::automock)]` generates mocks in test builds
- [x] Trait bounds include `Send + Sync` for async compatibility

---

## Phase 3: Envelope Service

### Overview
Implement the `EnvelopeService` that orchestrates the conversion flow.

### Changes Required

#### 1. Create EnvelopeService
**File**: `crates/ponix-domain/src/envelope_service.rs` (new file)

```rust
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
pub struct EnvelopeService {
    device_repository: Arc<dyn DeviceRepository>,
    payload_converter: Arc<dyn PayloadConverter>,
    producer: Arc<dyn ProcessedEnvelopeProducer>,
}

impl EnvelopeService {
    /// Create a new EnvelopeService with dependencies
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
    use crate::repository::MockDeviceRepository;
    use crate::repository::MockProcessedEnvelopeProducer;
    use crate::payload_converter::MockPayloadConverter;
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

        let service = EnvelopeService::new(
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

        let service = EnvelopeService::new(
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

        let service = EnvelopeService::new(
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

        let service = EnvelopeService::new(
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
        assert!(matches!(result, Err(DomainError::PayloadConversionError(_))));
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
            payload_conversion: "42".to_string(), // Returns a number, not object
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

        let service = EnvelopeService::new(
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
        assert!(matches!(result, Err(DomainError::PayloadConversionError(_))));
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

        mock_producer
            .expect_publish()
            .times(1)
            .return_once(|_| {
                Err(DomainError::RepositoryError(anyhow::anyhow!(
                    "NATS publish failed"
                )))
            });

        let service = EnvelopeService::new(
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
```

#### 2. Update Module Exports
**File**: `crates/ponix-domain/src/lib.rs`
**Changes**:

Add module declaration (after line 1):
```rust
pub mod envelope_service;
```

Add export (after line 6):
```rust
pub use envelope_service::EnvelopeService;
```

### Success Criteria

#### Automated Verification:
- [x] Code compiles: `cargo build -p ponix-domain`
- [x] All unit tests pass: `cargo test -p ponix-domain`
- [x] No clippy warnings: `cargo clippy -p ponix-domain`
- [x] Test coverage for all error paths

#### Manual Verification:
- [x] Service follows same pattern as `DeviceService`
- [x] Logging provides appropriate visibility into processing flow
- [x] Error messages are descriptive and actionable
- [x] Timestamp generation happens at correct point in flow

---

## Phase 4: CEL PayloadConverter Implementation

### Overview
Implement the `PayloadConverter` trait using the existing `CelEnvironment`.

### Changes Required

#### 1. Create CelPayloadConverter
**File**: `crates/ponix-payload/src/cel_converter.rs` (new file)

```rust
use ponix_domain::error::{DomainError, DomainResult};
use ponix_domain::PayloadConverter;
use crate::cel::CelEnvironment;
use tracing::{debug, error};

/// Implementation of PayloadConverter using CelEnvironment
///
/// Converts binary payloads to JSON using CEL expressions
pub struct CelPayloadConverter {
    cel_env: CelEnvironment,
}

impl CelPayloadConverter {
    /// Create a new CelPayloadConverter
    ///
    /// Initializes a CelEnvironment with cayenne_lpp_decode function registered
    pub fn new() -> Self {
        Self {
            cel_env: CelEnvironment::new(),
        }
    }
}

impl Default for CelPayloadConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl PayloadConverter for CelPayloadConverter {
    fn convert(&self, expression: &str, payload: &[u8]) -> DomainResult<serde_json::Value> {
        debug!(
            expression = %expression,
            payload_size = payload.len(),
            "Converting payload with CEL expression"
        );

        self.cel_env
            .execute(expression, payload)
            .map_err(|e| {
                error!(
                    expression = %expression,
                    error = %e,
                    "CEL execution failed"
                );
                DomainError::PayloadConversionError(e.to_string())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cel_converter_simple_expression() {
        let converter = CelPayloadConverter::new();
        let result = converter.convert("{'result': 42}", &[]);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["result"], 42);
    }

    #[test]
    fn test_cel_converter_cayenne_lpp_decode() {
        let converter = CelPayloadConverter::new();

        // Cayenne LPP payload: channel 1, temperature sensor (0x67), value 27.2°C (0x0110)
        let payload = vec![0x01, 0x67, 0x01, 0x10];

        let result = converter.convert("cayenne_lpp_decode(input)", &payload);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.is_object());
        assert!(json.get("temperature_1").is_some());
    }

    #[test]
    fn test_cel_converter_invalid_expression() {
        let converter = CelPayloadConverter::new();
        let result = converter.convert("invalid syntax {[", &[]);

        assert!(result.is_err());
        assert!(matches!(result, Err(DomainError::PayloadConversionError(_))));
    }

    #[test]
    fn test_cel_converter_complex_transformation() {
        let converter = CelPayloadConverter::new();

        let payload = vec![0x01, 0x67, 0x01, 0x10]; // 27.2°C

        // Transform to Fahrenheit with metadata
        let expression = r#"
            {
                'temperature_celsius': cayenne_lpp_decode(input).temperature_1,
                'temperature_fahrenheit': cayenne_lpp_decode(input).temperature_1 * 9.0 / 5.0 + 32.0,
                'unit': 'fahrenheit'
            }
        "#;

        let result = converter.convert(expression, &payload);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.get("temperature_celsius").is_some());
        assert!(json.get("temperature_fahrenheit").is_some());
        assert_eq!(json["unit"], "fahrenheit");
    }

    #[test]
    fn test_cel_converter_default_constructor() {
        let converter = CelPayloadConverter::default();
        let result = converter.convert("{'test': true}", &[]);

        assert!(result.is_ok());
    }
}
```

#### 2. Update Module Exports
**File**: `crates/ponix-payload/src/lib.rs`
**Changes**:

Add module declaration (after line 2):
```rust
pub mod cel_converter;
```

Add export (after line 6):
```rust
pub use cel_converter::CelPayloadConverter;
```

#### 3. Update Dependencies
**File**: `crates/ponix-payload/Cargo.toml`
**Add**: Add ponix-domain dependency

```toml
[dependencies]
# ... existing dependencies
ponix-domain = { path = "../ponix-domain" }
```

### Success Criteria

#### Automated Verification:
- [x] Code compiles: `cargo build -p ponix-payload`
- [x] All unit tests pass: `cargo test -p ponix-payload`
- [x] No clippy warnings: `cargo clippy -p ponix-payload`
- [x] Tests cover success and error cases

#### Manual Verification:
- [x] CelEnvironment is properly initialized with cayenne_lpp_decode
- [x] Error messages from CEL execution are preserved
- [x] Complex CEL expressions work correctly

---

## Phase 5: Refactor NATS Producer to Implement Domain Trait

### Overview
Refactor the existing `ProcessedEnvelopeProducer` in `ponix-nats` to implement the domain trait.

### Changes Required

#### 1. Add ponix-domain Dependency
**File**: `crates/ponix-nats/Cargo.toml`
**Location**: Under `[dependencies]`, in the `processed-envelope` feature section (around line 30)

```toml
[dependencies]
# ... existing dependencies

[dependencies.ponix-domain]
path = "../ponix-domain"
optional = true

[features]
# ... existing features
processed-envelope = ["protobuf", "ponix-proto", "ponix-domain", "prost-types", "serde_json", "chrono"]
```

#### 2. Refactor ProcessedEnvelopeProducer
**File**: `crates/ponix-nats/src/processed_envelope_producer.rs`
**Changes**: Add trait implementation

After the existing implementation (around line 60), add:

```rust
#[cfg(feature = "processed-envelope")]
use ponix_domain::repository::ProcessedEnvelopeProducer as ProcessedEnvelopeProducerTrait;
#[cfg(feature = "processed-envelope")]
use ponix_domain::types::ProcessedEnvelope as DomainProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use ponix_domain::error::{DomainError, DomainResult};

#[cfg(feature = "processed-envelope")]
#[async_trait::async_trait]
impl ProcessedEnvelopeProducerTrait for ProcessedEnvelopeProducer {
    async fn publish(&self, envelope: &DomainProcessedEnvelope) -> DomainResult<()> {
        // Convert domain ProcessedEnvelope to protobuf
        let proto_envelope = crate::conversions::domain_to_proto_envelope(envelope);

        // Use existing publish method
        self.publish(&proto_envelope)
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))
    }
}
```

#### 3. Add Domain to Proto Conversion
**File**: `crates/ponix-nats/src/conversions.rs`
**Location**: Add at the end of the file (after line 76)

```rust
/// Convert domain ProcessedEnvelope to protobuf ProcessedEnvelope
#[cfg(feature = "processed-envelope")]
pub fn domain_to_proto_envelope(
    envelope: &ponix_domain::types::ProcessedEnvelope,
) -> ponix_proto::processed_envelope::v1::ProcessedEnvelope {
    use prost_types::Timestamp;

    ponix_proto::processed_envelope::v1::ProcessedEnvelope {
        organization_id: envelope.organization_id.clone(),
        end_device_id: envelope.end_device_id.clone(),
        occurred_at: Some(Timestamp {
            seconds: envelope.occurred_at.timestamp(),
            nanos: envelope.occurred_at.timestamp_subsec_nanos() as i32,
        }),
        processed_at: Some(Timestamp {
            seconds: envelope.processed_at.timestamp(),
            nanos: envelope.processed_at.timestamp_subsec_nanos() as i32,
        }),
        data: Some(json_map_to_prost_struct(&envelope.data)),
    }
}

/// Convert serde_json::Map to prost_types::Struct
#[cfg(feature = "processed-envelope")]
fn json_map_to_prost_struct(
    map: &serde_json::Map<String, serde_json::Value>,
) -> prost_types::Struct {
    use prost_types::{value::Kind, Struct, Value};

    let fields = map
        .iter()
        .map(|(key, value)| {
            let prost_value = json_to_prost_value(value);
            (key.clone(), prost_value)
        })
        .collect();

    Struct { fields }
}

/// Convert serde_json::Value to prost_types::Value
#[cfg(feature = "processed-envelope")]
fn json_to_prost_value(value: &serde_json::Value) -> prost_types::Value {
    use prost_types::{value::Kind, ListValue, Struct, Value};

    let kind = match value {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Kind::NumberValue(i as f64)
            } else if let Some(u) = n.as_u64() {
                Kind::NumberValue(u as f64)
            } else if let Some(f) = n.as_f64() {
                Kind::NumberValue(f)
            } else {
                Kind::NullValue(0)
            }
        }
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => {
            let values = arr.iter().map(json_to_prost_value).collect();
            Kind::ListValue(ListValue { values })
        }
        serde_json::Value::Object(map) => Kind::StructValue(json_map_to_prost_struct(map)),
    };

    Value { kind: Some(kind) }
}
```

#### 4. Add Tests for Domain Trait Implementation
**File**: `crates/ponix-nats/src/processed_envelope_producer.rs`
**Location**: Add to test module (after existing tests)

```rust
#[cfg(all(test, feature = "processed-envelope"))]
mod domain_trait_tests {
    use super::*;
    use ponix_domain::repository::ProcessedEnvelopeProducer as ProcessedEnvelopeProducerTrait;
    use ponix_domain::types::ProcessedEnvelope as DomainProcessedEnvelope;
    use crate::traits::MockJetStreamPublisher;

    #[tokio::test]
    async fn test_domain_trait_publish_success() {
        // Arrange
        let mut mock_publisher = MockJetStreamPublisher::new();

        mock_publisher
            .expect_publish()
            .times(1)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_publisher),
            "test.stream".to_string(),
        );

        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));

        let envelope = DomainProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: chrono::Utc::now(),
            processed_at: chrono::Utc::now(),
            data,
        };

        // Act
        let result = ProcessedEnvelopeProducerTrait::publish(&producer, &envelope).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_domain_trait_publish_error() {
        // Arrange
        let mut mock_publisher = MockJetStreamPublisher::new();

        mock_publisher
            .expect_publish()
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("NATS publish failed")));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_publisher),
            "test.stream".to_string(),
        );

        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));

        let envelope = DomainProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: chrono::Utc::now(),
            processed_at: chrono::Utc::now(),
            data,
        };

        // Act
        let result = ProcessedEnvelopeProducerTrait::publish(&producer, &envelope).await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result, Err(ponix_domain::error::DomainError::RepositoryError(_))));
    }
}
```

#### 5. Add Tests for Conversions
**File**: `crates/ponix-nats/src/conversions.rs`
**Location**: Add to test module (after line 76)

```rust
#[cfg(all(test, feature = "processed-envelope"))]
mod domain_conversion_tests {
    use super::*;
    use ponix_domain::types::ProcessedEnvelope as DomainProcessedEnvelope;

    #[test]
    fn test_domain_to_proto_envelope() {
        // Arrange
        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));
        data.insert("humidity".to_string(), serde_json::json!(60));
        data.insert("active".to_string(), serde_json::json!(true));

        let occurred_at = chrono::DateTime::parse_from_rfc3339("2025-11-20T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let processed_at = chrono::DateTime::parse_from_rfc3339("2025-11-20T10:00:01Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let domain_envelope = DomainProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at,
            processed_at,
            data,
        };

        // Act
        let proto_envelope = domain_to_proto_envelope(&domain_envelope);

        // Assert
        assert_eq!(proto_envelope.organization_id, "org-123");
        assert_eq!(proto_envelope.end_device_id, "device-456");
        assert!(proto_envelope.occurred_at.is_some());
        assert!(proto_envelope.processed_at.is_some());
        assert!(proto_envelope.data.is_some());

        let data_struct = proto_envelope.data.unwrap();
        assert_eq!(data_struct.fields.len(), 3);
        assert!(data_struct.fields.contains_key("temperature"));
        assert!(data_struct.fields.contains_key("humidity"));
        assert!(data_struct.fields.contains_key("active"));
    }

    #[test]
    fn test_json_map_to_prost_struct() {
        // Arrange
        let mut map = serde_json::Map::new();
        map.insert("string_field".to_string(), serde_json::json!("test"));
        map.insert("number_field".to_string(), serde_json::json!(42.5));
        map.insert("bool_field".to_string(), serde_json::json!(true));
        map.insert("null_field".to_string(), serde_json::json!(null));

        // Act
        let prost_struct = json_map_to_prost_struct(&map);

        // Assert
        assert_eq!(prost_struct.fields.len(), 4);
    }

    #[test]
    fn test_json_to_prost_value_number() {
        let value = serde_json::json!(42);
        let prost_value = json_to_prost_value(&value);

        assert!(prost_value.kind.is_some());
        if let Some(prost_types::value::Kind::NumberValue(n)) = prost_value.kind {
            assert_eq!(n, 42.0);
        } else {
            panic!("Expected NumberValue");
        }
    }

    #[test]
    fn test_json_to_prost_value_string() {
        let value = serde_json::json!("hello");
        let prost_value = json_to_prost_value(&value);

        assert!(prost_value.kind.is_some());
        if let Some(prost_types::value::Kind::StringValue(s)) = prost_value.kind {
            assert_eq!(s, "hello");
        } else {
            panic!("Expected StringValue");
        }
    }

    #[test]
    fn test_json_to_prost_value_array() {
        let value = serde_json::json!([1, 2, 3]);
        let prost_value = json_to_prost_value(&value);

        assert!(prost_value.kind.is_some());
        if let Some(prost_types::value::Kind::ListValue(list)) = prost_value.kind {
            assert_eq!(list.values.len(), 3);
        } else {
            panic!("Expected ListValue");
        }
    }

    #[test]
    fn test_json_to_prost_value_nested_object() {
        let value = serde_json::json!({
            "nested": {
                "field": "value"
            }
        });
        let prost_value = json_to_prost_value(&value);

        assert!(prost_value.kind.is_some());
        if let Some(prost_types::value::Kind::StructValue(s)) = prost_value.kind {
            assert!(s.fields.contains_key("nested"));
        } else {
            panic!("Expected StructValue");
        }
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] Code compiles: `cargo build -p ponix-nats --features processed-envelope`
- [x] All tests pass: `cargo test -p ponix-nats --features processed-envelope`
- [x] No clippy warnings: `cargo clippy -p ponix-nats --features processed-envelope`
- [x] Domain trait implementation tests pass

#### Manual Verification:
- [x] Existing functionality is not broken
- [x] Conversion between domain and proto types is correct
- [x] Error propagation works as expected
- [x] Feature flag gates the domain integration properly

---

## Phase 6: Integration Testing

### Overview
Create integration tests that verify the complete conversion flow with all real implementations.

### Changes Required

#### 1. Create Integration Test File
**File**: `crates/ponix-domain/tests/envelope_integration_test.rs` (new file)

```rust
use ponix_domain::{
    EnvelopeService,
    RawEnvelope,
    DomainError,
};
use ponix_payload::CelPayloadConverter;
use std::sync::Arc;

// Mock implementations for integration testing
mod mocks {
    use super::*;
    use ponix_domain::{
        Device,
        GetDeviceInput,
        ProcessedEnvelope,
        repository::{DeviceRepository, ProcessedEnvelopeProducer},
        error::DomainResult,
    };
    use async_trait::async_trait;
    use std::sync::Mutex;

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

    pub struct InMemoryProducer {
        published: Mutex<Vec<ProcessedEnvelope>>,
    }

    impl InMemoryProducer {
        pub fn new() -> Self {
            Self {
                published: Mutex::new(Vec::new()),
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
```

### Success Criteria

#### Automated Verification:
- [x] All integration tests pass: `cargo test -p ponix-domain --test envelope_integration_test`
- [x] Tests verify complete flow with real CEL expressions
- [x] Error paths are fully covered

#### Manual Verification:
- [x] Integration tests demonstrate realistic usage patterns
- [x] All error scenarios produce appropriate error types
- [x] Tests serve as documentation for how to use the service

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that the implementation is complete before considering this feature done.

---

## Testing Strategy

### Unit Tests

Each component has comprehensive unit tests:
1. **EnvelopeService** ([ponix-domain/src/envelope_service.rs](crates/ponix-domain/src/envelope_service.rs))
   - Success case with mocked dependencies
   - Device not found
   - Missing/empty CEL expression
   - Conversion errors
   - Non-object JSON output
   - Publish errors

2. **CelPayloadConverter** ([ponix-payload/src/cel_converter.rs](crates/ponix-payload/src/cel_converter.rs))
   - Simple expressions
   - Cayenne LPP decoding
   - Complex transformations
   - Invalid expressions

3. **ProcessedEnvelopeProducer** ([ponix-nats/src/processed_envelope_producer.rs](crates/ponix-nats/src/processed_envelope_producer.rs))
   - Domain trait implementation
   - Publish success/failure
   - Domain-to-proto conversions

### Integration Tests

Integration tests verify end-to-end flow:
- Full conversion with Cayenne LPP
- Custom CEL transformations
- Device lookup errors
- CEL expression errors
- All failure paths

### Manual Testing Steps

After automated tests pass:
1. Create a device with `payload_conversion` field via gRPC
2. Manually invoke `EnvelopeService.process_raw_envelope()` with sample payload
3. Verify ProcessedEnvelope is published to NATS
4. Check logs for appropriate debug/info/error messages
5. Test with invalid CEL expressions
6. Test with missing device
7. Verify timestamp generation is correct

## Performance Considerations

### Synchronous CEL Execution
- CEL execution is CPU-bound, not I/O-bound
- `PayloadConverter.convert()` is synchronous (no async)
- This is correct because CEL doesn't perform I/O operations

### Individual Message Processing
- Service processes one envelope at a time (no batching)
- This supports per-message NAK in NATS consumers
- Future optimization: Parallel processing of independent messages

### Memory Usage
- Binary payloads held in memory during conversion
- JSON output held in memory before publish
- Acceptable for typical IoT payload sizes (<10KB)

## References

- Original ticket: [GitHub Issue #32](https://github.com/ponix-dev/ponix-rs/issues/32)
- Related implementations:
  - [DeviceService](crates/ponix-domain/src/end_device_service.rs) - Service pattern
  - [CelEnvironment](crates/ponix-payload/src/cel.rs#L134-L197) - CEL execution
  - [ProcessedEnvelopeProducer](crates/ponix-nats/src/processed_envelope_producer.rs) - NATS publishing
- Architecture docs: [CLAUDE.md](CLAUDE.md)
