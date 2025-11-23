# RawEnvelope NATS Producer and Consumer Implementation Plan

## Overview

Implement NATS producer and consumer infrastructure for the new `RawEnvelope` message type. This implementation follows the same architectural patterns established for `ProcessedEnvelope`, creating the foundation for ingesting raw binary payloads from IoT devices. The consumer will use the existing `EnvelopeService` to convert RawEnvelope → ProcessedEnvelope using CEL expressions.

## Current State Analysis

### What Exists Now:
- `RawEnvelope` domain type defined in [crates/ponix-domain/src/types.rs:62-69](crates/ponix-domain/src/types.rs#L62-L69)
- `EnvelopeService` in [crates/ponix-domain/src/envelope_service.rs:15-113](crates/ponix-domain/src/envelope_service.rs#L15-L113) that will be renamed to `RawEnvelopeService`
  - Takes dependencies: `DeviceRepository`, `PayloadConverter`, `ProcessedEnvelopeProducer`
  - Method: `process_raw_envelope(raw: RawEnvelope) -> DomainResult<()>`
  - Converts RawEnvelope → ProcessedEnvelope using CEL expressions
- Established pattern with ProcessedEnvelope:
  - Domain service: `ProcessedEnvelopeService` [crates/ponix-domain/src/processed_envelope_service.rs](crates/ponix-domain/src/processed_envelope_service.rs)
  - NATS producer: `ProcessedEnvelopeProducer` [crates/ponix-nats/src/processed_envelope_producer.rs](crates/ponix-nats/src/processed_envelope_producer.rs)
  - NATS consumer processor: `create_domain_processor()` [crates/ponix-nats/src/processed_envelope_processor.rs](crates/ponix-nats/src/processed_envelope_processor.rs)
  - Feature flag: `processed-envelope` [crates/ponix-nats/Cargo.toml:32](crates/ponix-nats/Cargo.toml#L32)
  - Demo producer: `run_demo_producer()` [crates/ponix-nats/src/demo_producer.rs](crates/ponix-nats/src/demo_producer.rs)

### What's Missing:
- Rename `EnvelopeService` → `RawEnvelopeService` in domain layer
- `RawEnvelopeProducer` trait in domain repository module
- NATS producer implementation for RawEnvelope
- NATS consumer processor that calls `RawEnvelopeService.process_raw_envelope()`
- Feature flag for raw-envelope functionality
- Integration in ponix-all-in-one
- Configuration for raw envelope streams/subjects

### Key Discoveries:
- `EnvelopeService` exists and will be renamed to `RawEnvelopeService` for clarity
- Domain types use `chrono::DateTime<Utc>` for timestamps
- NATS feature flags follow hierarchy: `protobuf` → `processed-envelope` pattern [crates/ponix-nats/Cargo.toml:27-32](crates/ponix-nats/Cargo.toml#L27-L32)
- Producer uses subject pattern: `{base_subject}.{end_device_id}` [crates/ponix-nats/src/processed_envelope_producer.rs:69](crates/ponix-nats/src/processed_envelope_producer.rs#L69)
- Processor returns `ProcessingResult` with per-message ack/nak indices [crates/ponix-nats/src/consumer.rs:43-71](crates/ponix-nats/src/consumer.rs#L43-L71)
- Domain services take `Arc<dyn Trait>` dependencies for repositories
- Integration in main.rs follows strict sequential initialization phases [crates/ponix-all-in-one/src/main.rs:44-178](crates/ponix-all-in-one/src/main.rs#L44-L178)

## Desired End State

### Success Criteria:

#### Automated Verification:
- [ ] All unit tests pass: `cargo test --workspace --lib --bins`
- [ ] Integration tests pass: `cargo test --workspace --features integration-tests`
- [ ] Linting passes: `cargo clippy --workspace --all-targets`
- [ ] Formatting is correct: `cargo fmt --check`
- [ ] Build succeeds with feature flag enabled: `cargo build --features raw-envelope`
- [ ] Build succeeds with feature flag disabled: `cargo build`

#### Manual Verification:
- [ ] Producer publishes RawEnvelope messages to NATS stream
- [ ] Consumer receives RawEnvelope messages and calls EnvelopeService
- [ ] EnvelopeService converts RawEnvelope → ProcessedEnvelope
- [ ] ProcessedEnvelope is published to processed_envelopes stream
- [ ] NATS streams exist: `docker exec -it ponix-nats nats stream ls`
- [ ] Messages flow: raw_envelopes → EnvelopeService → processed_envelopes
- [ ] No errors in service logs during normal operation

**Implementation Note**: After completing each phase and all automated verification passes, pause for manual confirmation before proceeding to the next phase.

## What We're NOT Doing

- Creating a new domain service (renaming existing `EnvelopeService` to `RawEnvelopeService`)
- Storage of RawEnvelope messages to database
- HTTP/gRPC API endpoints for submitting RawEnvelope messages
- Authentication or authorization for NATS streams
- Changes to the service's conversion logic (it already handles RawEnvelope → ProcessedEnvelope correctly)

## Implementation Approach

Follow the established ProcessedEnvelope pattern:
1. Rename `EnvelopeService` to `RawEnvelopeService` in domain layer
2. Domain layer defines producer trait for RawEnvelope
3. NATS layer implements producer trait with feature-gated modules
4. NATS consumer processor calls `RawEnvelopeService.process_raw_envelope()`
5. Conversions handle domain ↔ protobuf serialization
6. Integration layer wires everything together with Runner

Key architectural principles:
- **Domain-Driven Design**: Domain layer owns business logic and defines interfaces
- **Dependency Inversion**: Infrastructure implements domain traits
- **Feature Flags**: Optional compilation for flexibility
- **Fine-Grained Error Handling**: Per-message ack/nak control

---

## Phase 0: Rename EnvelopeService to RawEnvelopeService

### Overview
Rename the existing `EnvelopeService` to `RawEnvelopeService` for better clarity. This service handles the conversion of RawEnvelope → ProcessedEnvelope.

### Changes Required:

#### 1. Rename Service File
**Files**:
- Rename `crates/ponix-domain/src/envelope_service.rs` → `crates/ponix-domain/src/raw_envelope_service.rs`

**Changes**: Update struct name and all references

```rust
// Change struct name (line 15)
pub struct RawEnvelopeService {
    device_repository: Arc<dyn DeviceRepository>,
    payload_converter: Arc<dyn PayloadConverter>,
    producer: Arc<dyn ProcessedEnvelopeProducer>,
}

// Update impl block (line 21)
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
        // ... existing implementation unchanged
    }
}

// Update all test struct names (lines 115-416)
#[cfg(test)]
mod tests {
    use super::*;
    // ... update EnvelopeService → RawEnvelopeService in all tests
}
```

#### 2. Update Module Exports
**File**: `crates/ponix-domain/src/lib.rs`

**Changes**: Update module name and export

```rust
// Change module declaration (line 2)
mod raw_envelope_service;

// Change export (line 7)
pub use raw_envelope_service::RawEnvelopeService;
```

### Success Criteria:

#### Automated Verification:
- [x] Domain layer builds: `cargo build -p ponix-domain`
- [x] Unit tests pass: `cargo test -p ponix-domain`
- [x] All tests in `raw_envelope_service.rs` pass
- [x] Clippy passes: `cargo clippy -p ponix-domain`
- [x] Formatting correct: `cargo fmt --check -p ponix-domain`

#### Manual Verification:
- [ ] Service can be instantiated with new name
- [ ] All tests still pass with the rename
- [ ] No references to old `EnvelopeService` name remain in domain crate

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation before proceeding to Phase 1.

---

## Phase 1: Domain Layer - RawEnvelopeProducer Trait

### Overview
Add the producer trait to the domain layer.

### Changes Required:

#### 1. Domain Repository Trait
**File**: `crates/ponix-domain/src/repository.rs`

**Changes**: Add `RawEnvelopeProducer` trait after `ProcessedEnvelopeProducer`

```rust
/// Repository trait for publishing raw envelopes to message broker
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait RawEnvelopeProducer: Send + Sync {
    /// Publish a single raw envelope
    async fn publish(&self, envelope: &crate::types::RawEnvelope) -> DomainResult<()>;
}
```

**Location**: After line 48 (after `ProcessedEnvelopeProducer`)

#### 2. Domain Module Exports
**File**: `crates/ponix-domain/src/lib.rs`

**Changes**: Add public export for the trait

```rust
// Add to exports section after line 13
pub use repository::RawEnvelopeProducer;
```

### Success Criteria:

#### Automated Verification:
- [x] Domain layer builds: `cargo build -p ponix-domain`
- [x] Unit tests pass: `cargo test -p ponix-domain`
- [x] Clippy passes: `cargo clippy -p ponix-domain`
- [x] Formatting correct: `cargo fmt --check -p ponix-domain`

#### Manual Verification:
- [x] Mock trait `MockRawEnvelopeProducer` is available for testing
- [x] Trait can be used in type signatures

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation before proceeding to Phase 2.

---

## Phase 2: NATS Producer Implementation

### Overview
Implement the NATS producer for RawEnvelope with feature flag, following the ProcessedEnvelopeProducer pattern.

### Changes Required:

#### 1. Feature Flag Configuration
**File**: `crates/ponix-nats/Cargo.toml`

**Changes**: Add `raw-envelope` feature flag

```toml
[features]
default = []
# Enable generic protobuf batch processor
protobuf = ["prost"]
# Enable ProcessedEnvelope-specific producer and processor
processed-envelope = ["ponix-proto", "protobuf", "xid", "ponix-domain", "prost-types", "serde_json", "chrono"]
# Enable RawEnvelope-specific producer and processor
raw-envelope = ["ponix-proto", "protobuf", "ponix-domain", "prost-types", "chrono"]
```

**Location**: After line 32

#### 2. Producer Implementation
**File**: `crates/ponix-nats/src/raw_envelope_producer.rs` (new file)

**Changes**: Create producer implementation

```rust
use crate::traits::JetStreamPublisher;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ponix_domain::error::{DomainError, DomainResult};
use ponix_domain::types::RawEnvelope as DomainRawEnvelope;
use ponix_domain::RawEnvelopeProducer as RawEnvelopeProducerTrait;
use ponix_proto::raw_envelope::v1::RawEnvelope as ProtoRawEnvelope;
use prost::Message as ProstMessage;
use std::sync::Arc;
use tracing::{debug, info};

/// NATS JetStream producer for RawEnvelope messages
#[cfg(feature = "raw-envelope")]
pub struct RawEnvelopeProducer {
    jetstream: Arc<dyn JetStreamPublisher>,
    base_subject: String,
}

#[cfg(feature = "raw-envelope")]
impl RawEnvelopeProducer {
    pub fn new(jetstream: Arc<dyn JetStreamPublisher>, base_subject: String) -> Self {
        info!(
            "Created RawEnvelopeProducer with base subject: {}",
            base_subject
        );
        Self {
            jetstream,
            base_subject,
        }
    }
}

#[cfg(feature = "raw-envelope")]
#[async_trait]
impl RawEnvelopeProducerTrait for RawEnvelopeProducer {
    async fn publish(&self, envelope: &DomainRawEnvelope) -> DomainResult<()> {
        // Convert domain RawEnvelope to protobuf
        let proto_envelope = crate::raw_envelope_conversions::domain_to_proto(envelope);

        // Serialize protobuf message
        let payload = proto_envelope.encode_to_vec();

        // Build subject: {base_subject}.{end_device_id}
        let subject = format!("{}.{}", self.base_subject, proto_envelope.end_device_id);

        debug!(
            subject = %subject,
            end_device_id = %proto_envelope.end_device_id,
            size_bytes = payload.len(),
            "Publishing RawEnvelope"
        );

        // Publish via JetStream
        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .context("Failed to publish and acknowledge message")
            .map_err(DomainError::RepositoryError)?;

        info!(
            subject = %subject,
            end_device_id = %proto_envelope.end_device_id,
            "Successfully published RawEnvelope"
        );

        Ok(())
    }
}

#[cfg(all(test, feature = "raw-envelope"))]
mod tests {
    use super::*;
    use crate::traits::MockJetStreamPublisher;
    use bytes::Bytes;

    #[tokio::test]
    async fn test_publish_success() {
        // Arrange
        let mut mock_jetstream = MockJetStreamPublisher::new();

        mock_jetstream
            .expect_publish()
            .withf(|subject: &String, _payload: &Bytes| {
                subject.starts_with("raw_envelopes.device-")
            })
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));

        let producer = RawEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "raw_envelopes".to_string(),
        );

        let envelope = DomainRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x02, 0x03],
        };

        // Act
        let result = producer.publish(&envelope).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_failure() {
        // Arrange
        let mut mock_jetstream = MockJetStreamPublisher::new();

        mock_jetstream
            .expect_publish()
            .times(1)
            .returning(|_, _| {
                Box::pin(async { Err(anyhow::anyhow!("NATS publish failed")) })
            });

        let producer = RawEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "raw_envelopes".to_string(),
        );

        let envelope = DomainRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: chrono::Utc::now(),
            payload: vec![0x01, 0x02, 0x03],
        };

        // Act
        let result = producer.publish(&envelope).await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result, Err(DomainError::RepositoryError(_))));
    }
}
```

#### 3. Conversion Functions
**File**: `crates/ponix-nats/src/raw_envelope_conversions.rs` (new file)

**Changes**: Implement domain ↔ protobuf conversions

```rust
use anyhow::{anyhow, Context, Result};
use ponix_domain::types::RawEnvelope as DomainRawEnvelope;
use ponix_proto::raw_envelope::v1::RawEnvelope as ProtoRawEnvelope;
use prost_types::Timestamp;

/// Convert protobuf RawEnvelope to domain RawEnvelope
#[cfg(feature = "raw-envelope")]
pub fn proto_to_domain(proto: ProtoRawEnvelope) -> Result<DomainRawEnvelope> {
    let occurred_at = timestamp_to_datetime(
        proto
            .occurred_at
            .ok_or_else(|| anyhow!("Missing occurred_at timestamp"))?,
    )?;

    Ok(DomainRawEnvelope {
        organization_id: proto.organization_id,
        end_device_id: proto.end_device_id,
        occurred_at,
        payload: proto.payload,
    })
}

/// Convert domain RawEnvelope to protobuf RawEnvelope
#[cfg(feature = "raw-envelope")]
pub fn domain_to_proto(envelope: &DomainRawEnvelope) -> ProtoRawEnvelope {
    ProtoRawEnvelope {
        organization_id: envelope.organization_id.clone(),
        end_device_id: envelope.end_device_id.clone(),
        occurred_at: Some(Timestamp {
            seconds: envelope.occurred_at.timestamp(),
            nanos: envelope.occurred_at.timestamp_subsec_nanos() as i32,
        }),
        payload: envelope.payload.clone(),
    }
}

/// Convert protobuf Timestamp to chrono DateTime
fn timestamp_to_datetime(ts: Timestamp) -> Result<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;

    chrono::Utc
        .timestamp_opt(ts.seconds, ts.nanos as u32)
        .single()
        .ok_or_else(|| anyhow!("Invalid timestamp: {} seconds, {} nanos", ts.seconds, ts.nanos))
}

#[cfg(all(test, feature = "raw-envelope"))]
mod tests {
    use super::*;

    #[test]
    fn test_proto_to_domain_success() {
        let now = chrono::Utc::now();
        let proto = ProtoRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Some(Timestamp {
                seconds: now.timestamp(),
                nanos: now.timestamp_subsec_nanos() as i32,
            }),
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = proto_to_domain(proto.clone());
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, "org-123");
        assert_eq!(domain.end_device_id, "device-456");
        assert_eq!(domain.payload, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_proto_to_domain_missing_timestamp() {
        let proto = ProtoRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: None,
            payload: vec![0x01, 0x02, 0x03],
        };

        let result = proto_to_domain(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_domain_to_proto_success() {
        let now = chrono::Utc::now();
        let domain = DomainRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: now,
            payload: vec![0x01, 0x02, 0x03],
        };

        let proto = domain_to_proto(&domain);
        assert_eq!(proto.organization_id, "org-123");
        assert_eq!(proto.end_device_id, "device-456");
        assert_eq!(proto.payload, vec![0x01, 0x02, 0x03]);
        assert!(proto.occurred_at.is_some());
    }

    #[test]
    fn test_round_trip_conversion() {
        let now = chrono::Utc::now();
        let original = DomainRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: now,
            payload: vec![0x01, 0x02, 0x03],
        };

        let proto = domain_to_proto(&original);
        let result = proto_to_domain(proto);
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, original.organization_id);
        assert_eq!(domain.end_device_id, original.end_device_id);
        assert_eq!(domain.payload, original.payload);
        // Note: timestamp precision may differ slightly due to conversion
        assert_eq!(domain.occurred_at.timestamp(), original.occurred_at.timestamp());
    }
}
```

#### 4. Module Exports
**File**: `crates/ponix-nats/src/lib.rs`

**Changes**: Add module declarations and exports

```rust
// Add after line 15 (after processed_envelope modules)
#[cfg(feature = "raw-envelope")]
mod raw_envelope_conversions;
#[cfg(feature = "raw-envelope")]
mod raw_envelope_producer;

// Add to exports section after line 28
#[cfg(feature = "raw-envelope")]
pub use raw_envelope_conversions::{domain_to_proto as raw_envelope_domain_to_proto, proto_to_domain as raw_envelope_proto_to_domain};
#[cfg(feature = "raw-envelope")]
pub use raw_envelope_producer::RawEnvelopeProducer;
```

### Success Criteria:

#### Automated Verification:
- [x] NATS crate builds with feature: `cargo build -p ponix-nats --features raw-envelope`
- [x] NATS crate builds without feature: `cargo build -p ponix-nats`
- [x] Unit tests pass: `cargo test -p ponix-nats --features raw-envelope`
- [x] Clippy passes: `cargo clippy -p ponix-nats --features raw-envelope`
- [x] Formatting correct: `cargo fmt --check -p ponix-nats`

#### Manual Verification:
- [ ] Producer can be instantiated with NATS client
- [ ] Conversion functions correctly handle timestamps
- [ ] Round-trip conversion preserves data integrity

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation before proceeding to Phase 3.

---

## Phase 3: NATS Consumer Processor

### Overview
Create the consumer processor that receives RawEnvelope messages from NATS and invokes `EnvelopeService.process_raw_envelope()`.

### Changes Required:

#### 1. Consumer Processor Implementation
**File**: `crates/ponix-nats/src/raw_envelope_processor.rs` (new file)

**Changes**: Create processor that calls EnvelopeService for each message individually

```rust
use crate::consumer::{BatchProcessor, ProcessingResult};
use crate::raw_envelope_conversions::proto_to_domain;
use anyhow::Result;
use futures::future::BoxFuture;
use ponix_domain::RawEnvelopeService;
use ponix_proto::raw_envelope::v1::RawEnvelope as ProtoRawEnvelope;
use prost::Message as ProstMessage;
use std::sync::Arc;
use tracing::{debug, error};

/// Create a batch processor for RawEnvelope messages that invokes RawEnvelopeService
///
/// This processor:
/// 1. Receives a batch of messages from NATS
/// 2. For each message:
///    a. Decodes protobuf RawEnvelope
///    b. Converts to domain RawEnvelope
///    c. Calls RawEnvelopeService.process_raw_envelope() sequentially
///    d. If Ok, ack the message; if Err, nak the message
/// 3. Returns ProcessingResult with per-message ack/nak decisions
///
/// Note: Messages are processed sequentially (not in parallel) for simplicity.
/// Each message is independent - one failure doesn't affect others.
#[cfg(feature = "raw-envelope")]
pub fn create_domain_processor(service: Arc<RawEnvelopeService>) -> BatchProcessor {
    Box::new(move |messages: &[async_nats::jetstream::Message]| {
        let service = service.clone();

        let mut ack_indices = Vec::new();
        let mut nak_indices = Vec::new();

        // Process each message individually
        for (index, msg) in messages.iter().enumerate() {
            // Phase 1: Decode protobuf
            let proto_envelope = match ProtoRawEnvelope::decode(&msg.payload[..]) {
                Ok(proto) => proto,
                Err(e) => {
                    error!("Failed to decode protobuf message at index {}: {}", index, e);
                    nak_indices.push((index, Some(format!("Decode error: {}", e))));
                    continue;
                }
            };

            // Phase 2: Convert to domain type
            let domain_envelope = match proto_to_domain(proto_envelope) {
                Ok(envelope) => envelope,
                Err(e) => {
                    error!("Failed to convert protobuf to domain at index {}: {}", index, e);
                    nak_indices.push((index, Some(format!("Conversion error: {}", e))));
                    continue;
                }
            };

            // Store for async processing
            nak_indices.push((index, Some("Not yet processed".to_string())));
        }

        Box::pin(async move {
            if messages.is_empty() {
                debug!("No RawEnvelope messages to process");
                return Ok(ProcessingResult::new(vec![], vec![]));
            }

            let mut ack_indices = Vec::new();
            let mut nak_indices = Vec::new();

            // Process each message sequentially through RawEnvelopeService
            for (index, msg) in messages.iter().enumerate() {
                // Decode protobuf
                let proto_envelope = match ProtoRawEnvelope::decode(&msg.payload[..]) {
                    Ok(proto) => proto,
                    Err(e) => {
                        error!("Failed to decode protobuf message at index {}: {}", index, e);
                        nak_indices.push((index, Some(format!("Decode error: {}", e))));
                        continue;
                    }
                };

                // Convert to domain type
                let domain_envelope = match proto_to_domain(proto_envelope) {
                    Ok(envelope) => envelope,
                    Err(e) => {
                        error!("Failed to convert protobuf to domain at index {}: {}", index, e);
                        nak_indices.push((index, Some(format!("Conversion error: {}", e))));
                        continue;
                    }
                };

                // Process through RawEnvelopeService
                match service.process_raw_envelope(domain_envelope.clone()).await {
                    Ok(()) => {
                        debug!(
                            device_id = %domain_envelope.end_device_id,
                            "Successfully processed RawEnvelope"
                        );
                        ack_indices.push(index);
                    }
                    Err(e) => {
                        error!(
                            device_id = %domain_envelope.end_device_id,
                            error = %e,
                            "Failed to process RawEnvelope"
                        );
                        nak_indices.push((index, Some(format!("Processing error: {}", e))));
                    }
                }
            }

            debug!(
                ack_count = ack_indices.len(),
                nak_count = nak_indices.len(),
                "Completed RawEnvelope batch processing"
            );

            Ok(ProcessingResult::new(ack_indices, nak_indices))
        }) as BoxFuture<'static, Result<ProcessingResult>>
    })
}

#[cfg(all(test, feature = "raw-envelope"))]
mod tests {
    use super::*;
    // Note: Testing the processor requires mocking NATS messages which is complex
    // Better to test via integration tests with real NATS
    // Unit tests focus on conversion functions instead
}
```

#### 2. Demo Producer for RawEnvelope
**File**: `crates/ponix-nats/src/raw_envelope_demo_producer.rs` (new file)

**Changes**: Create demo producer for testing

```rust
use anyhow::Result;
use ponix_domain::types::RawEnvelope;
use ponix_domain::RawEnvelopeProducer as RawEnvelopeProducerTrait;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use xid;

/// Configuration for the RawEnvelope demo producer
#[derive(Debug, Clone)]
pub struct RawEnvelopeDemoProducerConfig {
    pub interval: Duration,
    pub organization_id: String,
    pub log_message: String,
}

/// Run a demo producer that continuously publishes sample RawEnvelope messages
///
/// This is useful for testing the consumer and verifying the end-to-end flow.
/// The producer generates random device IDs and sample binary payloads (Cayenne LPP format).
#[cfg(feature = "raw-envelope")]
pub async fn run_demo_producer<P>(
    ctx: CancellationToken,
    config: RawEnvelopeDemoProducerConfig,
    producer: P,
) -> Result<()>
where
    P: RawEnvelopeProducerTrait,
{
    info!("RawEnvelope demo producer service started");

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping RawEnvelope demo producer");
                break;
            }
            _ = tokio::time::sleep(config.interval) => {
                let id = xid::new();

                // Generate sample binary payload (Cayenne LPP temperature)
                // Format: Channel(1) + Type(temperature=0x67) + Value(0x0110 = 27.2°C)
                let payload = vec![0x01, 0x67, 0x01, 0x10];

                let envelope = RawEnvelope {
                    end_device_id: id.to_string(),
                    occurred_at: chrono::Utc::now(),
                    payload,
                    organization_id: config.organization_id.clone(),
                };

                match producer.publish(&envelope).await {
                    Ok(_) => {
                        debug!("{}", config.log_message);
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to publish RawEnvelope");
                    }
                }
            }
        }
    }

    info!("RawEnvelope demo producer service stopped gracefully");
    Ok(())
}
```

#### 3. Update Module Exports
**File**: `crates/ponix-nats/src/lib.rs`

**Changes**: Add processor and demo producer exports

```rust
// Add module declaration after line 17
#[cfg(feature = "raw-envelope")]
mod raw_envelope_processor;
#[cfg(feature = "raw-envelope")]
mod raw_envelope_demo_producer;

// Add to exports section after line 31
#[cfg(feature = "raw-envelope")]
pub use raw_envelope_processor::create_domain_processor as create_raw_envelope_processor;
#[cfg(feature = "raw-envelope")]
pub use raw_envelope_demo_producer::{run_demo_producer as run_raw_envelope_demo_producer, RawEnvelopeDemoProducerConfig};
```

#### 4. Add xid dependency
**File**: `crates/ponix-nats/Cargo.toml`

**Changes**: Add xid to raw-envelope feature dependencies

```toml
# Update feature to include xid
raw-envelope = ["ponix-proto", "protobuf", "ponix-domain", "prost-types", "chrono", "xid"]
```

### Success Criteria:

#### Automated Verification:
- [ ] NATS crate builds with feature: `cargo build -p ponix-nats --features raw-envelope`
- [ ] Unit tests pass: `cargo test -p ponix-nats --features raw-envelope`
- [ ] Clippy passes: `cargo clippy -p ponix-nats --features raw-envelope`

#### Manual Verification:
- [ ] Processor can be created with RawEnvelopeService
- [ ] Demo producer can be instantiated with config

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation before proceeding to Phase 4.

---

## Phase 4: Integration in ponix-all-in-one

### Overview
Wire up the RawEnvelope producer and consumer as runner processes in the main service.

### Changes Required:

#### 1. Configuration Updates
**File**: `crates/ponix-all-in-one/src/config.rs`

**Changes**: Add RawEnvelope-specific configuration fields

```rust
// Add fields to ServiceConfig struct after nats_batch_wait_secs (around line 40)
    /// NATS stream name for raw envelopes
    #[serde(default = "default_raw_nats_stream")]
    pub raw_nats_stream: String,

    /// NATS subject pattern for raw envelope consumer filter
    #[serde(default = "default_raw_nats_subject")]
    pub raw_nats_subject: String,

    /// Batch size for raw envelope consumer
    #[serde(default = "default_raw_nats_batch_size")]
    pub raw_nats_batch_size: usize,

    /// Max wait time for raw envelope batches in seconds
    #[serde(default = "default_raw_nats_batch_wait_secs")]
    pub raw_nats_batch_wait_secs: u64,

    /// Interval for raw envelope demo producer in seconds
    #[serde(default = "default_raw_interval_secs")]
    pub raw_interval_secs: u64,

    /// Log message for raw envelope demo producer
    #[serde(default = "default_raw_message")]
    pub raw_message: String,

// Add default functions at module level (around line 100)
fn default_raw_nats_stream() -> String {
    "raw_envelopes".to_string()
}

fn default_raw_nats_subject() -> String {
    "raw_envelopes.>".to_string()
}

fn default_raw_nats_batch_size() -> usize {
    30
}

fn default_raw_nats_batch_wait_secs() -> u64 {
    5
}

fn default_raw_interval_secs() -> u64 {
    5
}

fn default_raw_message() -> String {
    "Published RawEnvelope to NATS".to_string()
}
```

#### 2. Cargo Dependencies
**File**: `crates/ponix-all-in-one/Cargo.toml`

**Changes**: Enable raw-envelope feature flag

```toml
# Update ponix-nats dependency to include both features
ponix-nats = { path = "../ponix-nats", features = ["processed-envelope", "raw-envelope"] }
```

#### 3. Main Service Integration
**File**: `crates/ponix-all-in-one/src/main.rs`

**Changes**: Add RawEnvelope producer and consumer initialization

```rust
// Add after line 88 (after domain service creation, before ClickHouse section)
    // Initialize RawEnvelopeService for raw → processed conversion
    let payload_converter = Arc::new(ponix_payload::CelPayloadConverter::new());
    let raw_envelope_service = Arc::new(ponix_domain::RawEnvelopeService::new(
        Arc::new(device_repository.clone()),
        payload_converter,
        processed_envelope_producer.clone(), // Note: need to move producer creation earlier
    ));

// Refactor: Move ProcessedEnvelopeProducer creation before ClickHouse section
// (Move lines 175-177 to before envelope_service creation)

// Add after NATS consumer creation (around line 175)
    // PHASE 8: RawEnvelope infrastructure
    info!("Setting up RawEnvelope producer and consumer...");

    // Ensure raw envelopes stream exists
    if let Err(e) = nats_client.ensure_stream(&config.raw_nats_stream).await {
        error!("Failed to ensure raw envelopes stream exists: {}", e);
        std::process::exit(1);
    }

    // Create RawEnvelope producer
    let raw_publisher_client = nats_client.create_publisher_client();
    let raw_producer = ponix_nats::RawEnvelopeProducer::new(
        raw_publisher_client,
        config.raw_nats_stream.clone(),
    );

    // Create RawEnvelope processor (uses RawEnvelopeService)
    let raw_processor = ponix_nats::create_raw_envelope_processor(raw_envelope_service.clone());

    // Create RawEnvelope consumer
    let raw_consumer_client = nats_client.create_consumer_client();
    let raw_consumer = match ponix_nats::NatsConsumer::new(
        raw_consumer_client,
        &config.raw_nats_stream,
        "ponix-all-in-one-raw",
        &config.raw_nats_subject,
        config.raw_nats_batch_size,
        config.raw_nats_batch_wait_secs,
        raw_processor,
    )
    .await
    {
        Ok(consumer) => consumer,
        Err(e) => {
            error!("Failed to create RawEnvelope consumer: {}", e);
            std::process::exit(1);
        }
    };

    // RawEnvelope demo producer config
    let raw_demo_config = ponix_nats::RawEnvelopeDemoProducerConfig {
        interval: Duration::from_secs(config.raw_interval_secs),
        organization_id: "example-org".to_string(),
        log_message: config.raw_message.clone(),
    };

// Update Runner to include RawEnvelope processes (around line 200)
    let runner = Runner::new()
        // ProcessedEnvelope processes
        .with_app_process(move |ctx| {
            Box::pin(async move { run_demo_producer(ctx, demo_config, producer).await })
        })
        .with_app_process(move |ctx| Box::pin(async move { consumer.run(ctx).await }))
        // RawEnvelope processes
        .with_app_process(move |ctx| {
            Box::pin(async move {
                ponix_nats::run_raw_envelope_demo_producer(ctx, raw_demo_config, raw_producer)
                    .await
            })
        })
        .with_app_process(move |ctx| Box::pin(async move { raw_consumer.run(ctx).await }))
        // gRPC server
        .with_app_process({
            let service = device_service.clone();
            move |ctx| Box::pin(async move { run_grpc_server(grpc_config, service, ctx).await })
        })
        .with_closer(move || {
            Box::pin(async move {
                info!("Running cleanup tasks...");
                nats_client.close().await;
                info!("Cleanup complete");
                Ok(())
            })
        })
        .with_closer_timeout(Duration::from_secs(10));
```

**Important Refactoring Notes**:
- Move `ProcessedEnvelopeProducer` creation to before `EnvelopeService` initialization
- `EnvelopeService` needs the processed envelope producer as a dependency
- Ensure proper initialization order: repos → producers → services → consumers

### Success Criteria:

#### Automated Verification:
- [ ] Service builds: `cargo build -p ponix-all-in-one`
- [ ] No compiler warnings: `cargo build -p ponix-all-in-one 2>&1 | grep warning`
- [ ] Clippy passes: `cargo clippy -p ponix-all-in-one`

#### Manual Verification:
- [ ] Service starts without errors: `cargo run -p ponix-all-in-one`
- [ ] Logs show both streams created: "raw_envelopes" and "processed_envelopes"
- [ ] Logs show RawEnvelope demo producer publishing messages
- [ ] Logs show RawEnvelope consumer receiving messages
- [ ] Logs show RawEnvelopeService processing raw → processed
- [ ] Logs show ProcessedEnvelope messages being published
- [ ] Both NATS streams exist: `docker exec -it ponix-nats nats stream ls`
- [ ] Messages flow through both streams
- [ ] Service shuts down gracefully on SIGTERM

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation before proceeding to Phase 5.

---

## Phase 5: Testing and Documentation

### Overview
Add comprehensive tests and documentation for the new functionality.

### Changes Required:

#### 1. Integration Test
**File**: `crates/ponix-nats/tests/raw_envelope_integration_test.rs` (new file)

**Changes**: Add integration test with real NATS (if testcontainers support is available)

```rust
#[cfg(all(test, feature = "raw-envelope"))]
mod raw_envelope_tests {
    // Integration tests would go here
    // Note: Requires testcontainers setup similar to ClickHouse tests
}
```

#### 2. Update CLAUDE.md
**File**: `CLAUDE.md`

**Changes**: Document RawEnvelope flow in Architecture section

```markdown
## Architecture

### Data Flow

#### Raw Envelope Ingestion Flow
```
External System (future)
    ↓
RawEnvelope (binary payload) → NATS (raw_envelopes stream)
    ↓
RawEnvelope Consumer
    ↓
RawEnvelopeService.process_raw_envelope()
├─ DeviceRepository: Fetch device with CEL expression
├─ PayloadConverter: Execute CEL transformation on binary data
└─ ProcessedEnvelopeProducer: Publish to NATS (processed_envelopes stream)
    ↓
ProcessedEnvelope → ClickHouse
```

This flow demonstrates:
- **Separation of Concerns**: Raw ingestion separate from processing
- **CEL-based Transformation**: Flexible payload conversion per device
- **Event-Driven Architecture**: NATS as message broker between stages
```

#### 3. Update README (if exists)
**File**: `README.md` or relevant documentation

**Changes**: Document the new RawEnvelope feature

```markdown
### RawEnvelope Processing

The system supports ingesting raw binary payloads from IoT devices:

1. **Producer** publishes RawEnvelope with binary payload to NATS
2. **Consumer** receives RawEnvelope and calls RawEnvelopeService
3. **RawEnvelopeService** converts binary → JSON using device's CEL expression
4. **ProcessedEnvelope** is published to separate stream for storage

Configuration:
- `PONIX_RAW_NATS_STREAM`: Stream name (default: "raw_envelopes")
- `PONIX_RAW_NATS_SUBJECT`: Subject filter (default: "raw_envelopes.>")
- `PONIX_RAW_INTERVAL_SECS`: Demo producer interval (default: 5)
```

### Success Criteria:

#### Automated Verification:
- [ ] All tests pass: `cargo test --workspace`
- [ ] Documentation builds: `cargo doc --no-deps --workspace`
- [ ] No broken links in documentation

#### Manual Verification:
- [ ] End-to-end flow documented clearly
- [ ] Configuration options are clear
- [ ] Architecture diagrams reflect RawEnvelope flow

**Implementation Note**: After completing this phase and all verification passes, the implementation is complete.

---

## Testing Strategy

### Unit Tests:
- **Conversions**: Test proto ↔ domain conversions with various edge cases (timestamps, empty payloads, etc.)
- **Producer**: Mock JetStream client and verify publish calls with correct subject pattern
- **Processor**: Verify decode/conversion phases work correctly

### Integration Tests:
- **End-to-End Flow**: RawEnvelope → NATS → Consumer → EnvelopeService → ProcessedEnvelope
- **Error Handling**: Test decode failures, device not found, CEL errors, publish failures
- **Graceful Shutdown**: Verify consumer stops cleanly on cancellation

### Manual Testing Steps:
1. Start infrastructure: `docker-compose -f docker/docker-compose.deps.yaml up -d`
2. Create a test device with CEL expression:
   ```bash
   grpcurl -plaintext -d '{
     "organization_id": "example-org",
     "name": "Test Sensor",
     "payload_conversion": "cayenne_lpp_decode(input)"
   }' localhost:50051 ponix.end_device.v1.EndDeviceService/CreateEndDevice
   ```
3. Run service: `cargo run -p ponix-all-in-one`
4. Verify logs show:
   - "Created RawEnvelopeProducer with base subject: raw_envelopes"
   - "RawEnvelope demo producer service started"
   - "Processing raw envelope" (from RawEnvelopeService)
   - "Successfully processed and published envelope"
   - "Successfully processed envelope batch" (from ProcessedEnvelope consumer)
5. Check both NATS streams exist:
   ```bash
   docker exec -it ponix-nats nats stream ls
   # Should show: raw_envelopes, processed_envelopes
   ```
6. View raw messages: `docker exec -it ponix-nats nats stream view raw_envelopes`
7. View processed messages: `docker exec -it ponix-nats nats stream view processed_envelopes`
8. Check ClickHouse for stored envelopes:
   ```bash
   docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix
   SELECT * FROM ponix.processed_envelopes ORDER BY processed_at DESC LIMIT 5;
   ```
9. Send SIGTERM: `kill -TERM <pid>` and verify graceful shutdown
10. Verify no error messages in logs

## Performance Considerations

- **Batch Processing**: Consumer processes messages in batches (default 30) for efficiency
- **Per-Message Ack/Nak**: Individual messages can fail without affecting the batch
- **Memory**: Binary payloads are cloned during conversion; acceptable for IoT use case (small payloads)
- **Serialization**: Protobuf encoding is efficient for binary data
- **Subject Pattern**: Using `{stream}.{device_id}` enables device-specific filtering in future
- **CEL Execution**: Payload conversion happens per-message; performance depends on CEL complexity

## Migration Notes

This is a new feature with no existing data to migrate. However:
- NATS stream `raw_envelopes` will be created automatically on first run
- Stream is durable and persists messages based on retention policy
- Consumer uses durable name `ponix-all-in-one-raw` for offset tracking
- No database schema changes required
- Existing ProcessedEnvelope flow continues unchanged

## Future Enhancements (Out of Scope)

- HTTP/gRPC endpoints for external systems to submit RawEnvelope messages
- Dead letter queue for failed conversions
- Metrics and monitoring for conversion success/failure rates
- Replay functionality for reprocessing raw envelopes
- Storage of RawEnvelope to database for audit trail

## References

- Original ticket: [ponix-dev/ponix-rs#30](https://github.com/ponix-dev/ponix-rs/issues/30)
- Related protobuf PR: ponix-dev/ponix-protobuf#5
- RawEnvelopeService implementation: [crates/ponix-domain/src/raw_envelope_service.rs](crates/ponix-domain/src/raw_envelope_service.rs)
- ProcessedEnvelope producer: [crates/ponix-nats/src/processed_envelope_producer.rs](crates/ponix-nats/src/processed_envelope_producer.rs)
- Runner pattern: [crates/runner/src/lib.rs](crates/runner/src/lib.rs)
- NATS consumer pattern: [crates/ponix-nats/src/consumer.rs](crates/ponix-nats/src/consumer.rs)
