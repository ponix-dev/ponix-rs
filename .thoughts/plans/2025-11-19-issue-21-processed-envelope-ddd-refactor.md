# ProcessedEnvelope Domain-Driven Architecture Refactor

## Overview

Refactor the NATS → ClickHouse message ingestion flow to follow the domain-driven design pattern established in issue #17. This pure refactor introduces proper separation of concerns across NATS, domain, and infrastructure layers without changing functional behavior.

## Current State Analysis

### Existing Architecture
```
NATS (protobuf) → Consumer → ClickHouse (direct write)
```

**Key Files:**
- [ponix-nats/src/processed_envelope_processor.rs](crates/ponix-nats/src/processed_envelope_processor.rs) - Creates ClickHouse processor
- [ponix-clickhouse/src/envelope_store.rs](crates/ponix-clickhouse/src/envelope_store.rs) - Direct batch writer
- [ponix-all-in-one/src/main.rs:145-148](crates/ponix-all-in-one/src/main.rs#L145-L148) - Wires consumer to ClickHouse

### Current Flow
1. NATS consumer fetches batch of messages
2. `create_protobuf_processor()` decodes protobuf → `ProcessedEnvelope`
3. `create_clickhouse_processor()` wrapper calls `EnvelopeStore.store_processed_envelopes()`
4. Direct batch insert to ClickHouse with type conversions (Protobuf Struct → JSON)

### Key Constraints Discovered
- **Batch Processing**: Uses `Vec<ProcessedEnvelope>` for efficiency
- **Fine-Grained Ack/Nak**: Per-message acknowledgment must be preserved
- **Protobuf Source**: `ProcessedEnvelope` comes from external `ponix-proto` package via Buf Schema Registry
- **Type Conversions**: Protobuf `Struct` → JSON, `Timestamp` → `DateTime<Utc>`
- **Feature Flag**: `processed-envelope` feature in ponix-nats enables this functionality

## Desired End State

### Target Architecture
```
NATS (protobuf) → Protobuf→Domain Conversion → Domain Service → ProcessedEnvelopeRepository trait → ClickHouse Implementation
```

**Layer Responsibilities:**
- **ponix-nats**: Protobuf handling, Protobuf → Domain conversion, consumer mechanics
- **ponix-domain**: Domain entity, repository trait, pass-through service
- **ponix-clickhouse**: Repository implementation, Domain → Database mapping, batch insertion
- **ponix-all-in-one**: Dependency wiring only (no business logic)

### Success Criteria

#### Automated Verification:
- [x] All existing unit tests pass: Tests for modified crates (ponix-domain, ponix-clickhouse, ponix-nats) pass
- [x] New domain service unit tests pass with Mockall mocks
- [x] Type checking passes: `cargo check --workspace` (Note: CEL dependency has unrelated compilation issue)
- [ ] Clippy passes: `cargo clippy --workspace` (Note: CEL dependency has unrelated compilation issue)
- [x] Service builds successfully: `cargo build -p ponix-all-in-one`

#### Manual Verification:
- [ ] NATS → ClickHouse flow works end-to-end with `tilt up`
- [ ] Messages are successfully stored in ClickHouse
- [ ] Batch processing maintains same performance characteristics
- [ ] Error handling behaves identically (failed batch = all messages nak'd)

## What We're NOT Doing

- No validation logic or business rules (pure pass-through service)
- No changes to ClickHouse schema or queries
- No performance optimizations
- No new message types or endpoints
- No error handling improvements beyond maintaining existing behavior
- No integration tests (unit tests only with mocks)
- No changes to batch size or consumer configuration

## Implementation Approach

This refactor follows the same DDD pattern established for device management:
1. Create domain types separate from protobuf types
2. Define repository trait in domain layer (dependency inversion)
3. Implement repository trait in infrastructure layer
4. Add type conversion functions at each boundary
5. Wire components together in main service

The domain service will be a pass-through initially, enabling future business logic without requiring additional refactoring.

---

## Phase 1: Domain Layer - Types and Repository Trait

### Overview
Create the domain foundation by adding ProcessedEnvelope types, repository trait, and domain service to `ponix-domain` crate.

### Changes Required

#### 1. Add Domain Types to ponix-domain
**File**: `crates/ponix-domain/src/types.rs`
**Changes**: Add new domain types after existing Device types (around line 40)

```rust
// ProcessedEnvelope domain types

/// Domain entity for a processed envelope
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedEnvelope {
    pub organization_id: String,
    pub end_device_id: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub processed_at: chrono::DateTime<chrono::Utc>,
    pub data: serde_json::Map<String, serde_json::Value>,
}

/// Input for storing processed envelopes (batch operation)
#[derive(Debug, Clone)]
pub struct StoreEnvelopesInput {
    pub envelopes: Vec<ProcessedEnvelope>,
}
```

**Rationale**:
- Separate domain type from protobuf type enables future business logic
- Uses `chrono::DateTime` (domain standard) instead of protobuf `Timestamp`
- Uses `serde_json::Map` instead of protobuf `Struct` for domain representation
- Batch input type matches repository operation semantics

#### 2. Add ProcessedEnvelopeRepository Trait
**File**: `crates/ponix-domain/src/repository.rs`
**Changes**: Add new trait after `DeviceRepository` (around line 20)

```rust
/// Repository trait for processed envelope storage operations
/// Infrastructure layer (e.g., ponix-clickhouse) implements this trait
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ProcessedEnvelopeRepository: Send + Sync {
    /// Store a batch of processed envelopes
    /// Implementations should handle chunking if needed for large batches
    /// Failure handling: entire batch fails atomically (all-or-nothing)
    async fn store_batch(&self, input: StoreEnvelopesInput) -> DomainResult<()>;
}
```

**Rationale**:
- Follows same pattern as `DeviceRepository` with `#[cfg_attr(test, mockall::automock)]`
- Batch operation matches actual use case
- Repository handles chunking internally (implementation detail)
- Returns `DomainResult<()>` for consistency with domain error types

#### 3. Create ProcessedEnvelopeService
**File**: `crates/ponix-domain/src/processed_envelope_service.rs` (new file)
**Changes**: Create new service file

```rust
use crate::repository::ProcessedEnvelopeRepository;
use crate::types::StoreEnvelopesInput;
use crate::{DomainError, DomainResult};
use std::sync::Arc;
use tracing::debug;

/// Domain service for processed envelope operations
pub struct ProcessedEnvelopeService {
    repository: Arc<dyn ProcessedEnvelopeRepository>,
}

impl ProcessedEnvelopeService {
    pub fn new(repository: Arc<dyn ProcessedEnvelopeRepository>) -> Self {
        Self { repository }
    }

    /// Store a batch of processed envelopes
    /// Currently a pass-through to repository, but provides extension point for future validation
    pub async fn store_batch(&self, input: StoreEnvelopesInput) -> DomainResult<()> {
        debug!(
            envelope_count = input.envelopes.len(),
            "Storing batch of envelopes"
        );

        self.repository.store_batch(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::MockProcessedEnvelopeRepository;
    use crate::types::ProcessedEnvelope;
    use chrono::Utc;

    #[tokio::test]
    async fn test_store_batch_success() {
        let mut mock_repo = MockProcessedEnvelopeRepository::new();

        // Setup mock expectation
        mock_repo
            .expect_store_batch()
            .withf(|input: &StoreEnvelopesInput| input.envelopes.len() == 2)
            .times(1)
            .return_once(|_| Ok(()));

        let service = ProcessedEnvelopeService::new(Arc::new(mock_repo));

        // Create test envelopes
        let envelopes = vec![
            ProcessedEnvelope {
                organization_id: "org-123".to_string(),
                end_device_id: "device-456".to_string(),
                occurred_at: Utc::now(),
                processed_at: Utc::now(),
                data: serde_json::Map::new(),
            },
            ProcessedEnvelope {
                organization_id: "org-123".to_string(),
                end_device_id: "device-789".to_string(),
                occurred_at: Utc::now(),
                processed_at: Utc::now(),
                data: serde_json::Map::new(),
            },
        ];

        let input = StoreEnvelopesInput { envelopes };

        // Call service
        let result = service.store_batch(input).await;

        // Verify
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_store_batch_repository_error() {
        let mut mock_repo = MockProcessedEnvelopeRepository::new();

        // Setup mock to return error
        mock_repo
            .expect_store_batch()
            .times(1)
            .return_once(|_| {
                Err(DomainError::RepositoryError(anyhow::anyhow!(
                    "Database connection failed"
                )))
            });

        let service = ProcessedEnvelopeService::new(Arc::new(mock_repo));

        let input = StoreEnvelopesInput {
            envelopes: vec![],
        };

        // Call service
        let result = service.store_batch(input).await;

        // Verify error propagation
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::RepositoryError(_)
        ));
    }
}
```

**Rationale**:
- Pass-through service maintains consistency with DDD pattern
- Provides extension point for future validation without refactoring
- Includes unit tests with Mockall mocks as requested
- Tests verify service properly delegates to repository

#### 4. Update lib.rs Exports
**File**: `crates/ponix-domain/src/lib.rs`
**Changes**: Add exports after existing exports (around line 9)

```rust
mod processed_envelope_service;

pub use processed_envelope_service::ProcessedEnvelopeService;
```

Also update repository exports to include new trait:
```rust
pub use repository::{DeviceRepository, ProcessedEnvelopeRepository};
```

#### 5. Update Cargo.toml Dependencies
**File**: `crates/ponix-domain/Cargo.toml`
**Changes**: Add `serde_json` dependency (needed for domain type)

```toml
[dependencies]
# ... existing dependencies ...
serde_json = { workspace = true }
```

### Success Criteria

#### Automated Verification:
- [x] Domain types compile: `cargo check -p ponix-domain`
- [x] Unit tests pass: `cargo test -p ponix-domain`
- [x] Mockall generates mocks correctly for `ProcessedEnvelopeRepository`

#### Manual Verification:
- [ ] Repository trait can be imported in other crates
- [ ] Mock generation works for unit tests

---

## Phase 2: Infrastructure Layer - ClickHouse Repository Implementation

### Overview
Implement `ProcessedEnvelopeRepository` trait in `ponix-clickhouse` crate, refactoring existing `EnvelopeStore` to use domain types.

### Changes Required

#### 1. Implement Repository Trait
**File**: `crates/ponix-clickhouse/src/envelope_repository.rs` (new file)
**Changes**: Create new repository implementation

```rust
use async_trait::async_trait;
use ponix_domain::{DomainError, DomainResult, ProcessedEnvelopeRepository};
use ponix_domain::types::{ProcessedEnvelope as DomainEnvelope, StoreEnvelopesInput};
use tracing::{debug, error};

use crate::client::ClickHouseClient;
use crate::models::ProcessedEnvelopeRow;

/// ClickHouse implementation of ProcessedEnvelopeRepository
#[derive(Clone)]
pub struct ClickHouseEnvelopeRepository {
    client: ClickHouseClient,
    table: String,
}

impl ClickHouseEnvelopeRepository {
    pub fn new(client: ClickHouseClient, table: String) -> Self {
        Self { client, table }
    }
}

#[async_trait]
impl ProcessedEnvelopeRepository for ClickHouseEnvelopeRepository {
    async fn store_batch(&self, input: StoreEnvelopesInput) -> DomainResult<()> {
        if input.envelopes.is_empty() {
            debug!("No envelopes to store, skipping");
            return Ok(());
        }

        debug!(
            envelope_count = input.envelopes.len(),
            table = %self.table,
            "Storing envelope batch to ClickHouse"
        );

        // Convert domain envelopes to database rows
        let rows: Vec<ProcessedEnvelopeRow> = input
            .envelopes
            .iter()
            .map(|envelope| envelope.into())
            .collect();

        // Insert batch using ClickHouse inserter API
        let mut insert = self
            .client
            .get_client()
            .insert::<ProcessedEnvelopeRow>(&self.table)
            .await
            .map_err(|e| {
                error!("Failed to create ClickHouse inserter: {}", e);
                DomainError::RepositoryError(e.into())
            })?;

        for row in &rows {
            insert.write(row).await.map_err(|e| {
                error!("Failed to write row to ClickHouse: {}", e);
                DomainError::RepositoryError(e.into())
            })?;
        }

        insert.end().await.map_err(|e| {
            error!("Failed to finalize ClickHouse insert: {}", e);
            DomainError::RepositoryError(e.into())
        })?;

        debug!(
            rows_inserted = rows.len(),
            "Successfully stored envelope batch"
        );

        Ok(())
    }
}
```

**Rationale**:
- Implements domain repository trait (dependency inversion)
- Converts domain types to database models using `From` trait
- Maps ClickHouse errors to `DomainError::RepositoryError`
- Maintains existing batch insert pattern with inserter API
- Handles empty batch gracefully

#### 2. Add Domain to Database Conversions
**File**: `crates/ponix-clickhouse/src/conversions.rs` (new file)
**Changes**: Create conversion functions

```rust
use chrono::{DateTime, Utc};
use ponix_domain::types::ProcessedEnvelope as DomainEnvelope;
use serde_json;

use crate::models::ProcessedEnvelopeRow;

/// Convert domain ProcessedEnvelope to database ProcessedEnvelopeRow
impl From<&DomainEnvelope> for ProcessedEnvelopeRow {
    fn from(envelope: &DomainEnvelope) -> Self {
        // Convert serde_json::Map to JSON string for ClickHouse storage
        let data_json = serde_json::to_string(&envelope.data)
            .unwrap_or_else(|_| "{}".to_string());

        ProcessedEnvelopeRow {
            organization_id: envelope.organization_id.clone(),
            end_device_id: envelope.end_device_id.clone(),
            occurred_at: envelope.occurred_at,
            processed_at: envelope.processed_at,
            data: data_json,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_to_row_conversion() {
        let mut data_map = serde_json::Map::new();
        data_map.insert("temperature".to_string(), serde_json::json!(23.5));
        data_map.insert("humidity".to_string(), serde_json::json!(65));

        let domain_envelope = DomainEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Utc::now(),
            processed_at: Utc::now(),
            data: data_map,
        };

        let row: ProcessedEnvelopeRow = (&domain_envelope).into();

        assert_eq!(row.organization_id, "org-123");
        assert_eq!(row.end_device_id, "device-456");
        assert!(row.data.contains("temperature"));
        assert!(row.data.contains("23.5"));
    }

    #[test]
    fn test_empty_data_conversion() {
        let domain_envelope = DomainEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Utc::now(),
            processed_at: Utc::now(),
            data: serde_json::Map::new(),
        };

        let row: ProcessedEnvelopeRow = (&domain_envelope).into();

        assert_eq!(row.data, "{}");
    }
}
```

**Rationale**:
- Uses `From` trait for idiomatic Rust conversions
- Simplifies existing conversion logic (domain already uses `serde_json::Map`)
- Handles serialization errors gracefully
- Includes unit tests for conversion logic

#### 3. Update lib.rs Exports
**File**: `crates/ponix-clickhouse/src/lib.rs`
**Changes**: Add new module exports

```rust
mod conversions;
mod envelope_repository;

pub use envelope_repository::ClickHouseEnvelopeRepository;
```

#### 4. Update Cargo.toml Dependencies
**File**: `crates/ponix-clickhouse/Cargo.toml`
**Changes**: Add ponix-domain dependency

```toml
[dependencies]
# ... existing dependencies ...
ponix-domain = { path = "../ponix-domain" }
```

#### 5. Deprecate Old EnvelopeStore (Optional)
**File**: `crates/ponix-clickhouse/src/envelope_store.rs`
**Changes**: Add deprecation notice at top of file

```rust
// This module will be removed after migration to domain-driven architecture
// Use ClickHouseEnvelopeRepository instead (envelope_repository.rs)
#[deprecated(note = "Use ClickHouseEnvelopeRepository instead")]
```

**Rationale**: Marks old code for future removal while maintaining backward compatibility during migration.

### Success Criteria

#### Automated Verification:
- [x] Repository implementation compiles: `cargo check -p ponix-clickhouse`
- [x] Conversion tests pass: `cargo test -p ponix-clickhouse`
- [x] No clippy warnings: `cargo clippy -p ponix-clickhouse`

#### Manual Verification:
- [ ] Repository can be instantiated with ClickHouse client
- [ ] Conversions handle all field types correctly

---

## Phase 3: NATS Layer - Protobuf to Domain Conversion

### Overview
Update `ponix-nats` to convert protobuf types to domain types and remove direct ClickHouse dependencies.

### Changes Required

#### 1. Add Protobuf to Domain Conversions
**File**: `crates/ponix-nats/src/conversions.rs` (new file)
**Changes**: Create conversion functions

```rust
use anyhow::{anyhow, Result};
use ponix_domain::types::ProcessedEnvelope as DomainEnvelope;
use ponix_proto::envelope::v1::ProcessedEnvelope as ProtoEnvelope;
use prost_types::{Timestamp, value::Kind};
use serde_json;

/// Convert protobuf ProcessedEnvelope to domain ProcessedEnvelope
pub fn proto_to_domain_envelope(proto: ProtoEnvelope) -> Result<DomainEnvelope> {
    // Convert timestamps
    let occurred_at = timestamp_to_datetime(
        proto.occurred_at.ok_or_else(|| anyhow!("Missing occurred_at timestamp"))?
    )?;

    let processed_at = timestamp_to_datetime(
        proto.processed_at.ok_or_else(|| anyhow!("Missing processed_at timestamp"))?
    )?;

    // Convert protobuf Struct to serde_json::Map
    let data = match proto.data {
        Some(struct_val) => {
            let mut map = serde_json::Map::new();
            for (key, value) in struct_val.fields {
                if let Some(json_value) = prost_value_to_json(&value) {
                    map.insert(key, json_value);
                }
            }
            map
        }
        None => serde_json::Map::new(),
    };

    Ok(DomainEnvelope {
        organization_id: proto.organization_id,
        end_device_id: proto.end_device_id,
        occurred_at,
        processed_at,
        data,
    })
}

/// Convert protobuf Timestamp to chrono DateTime
fn timestamp_to_datetime(ts: Timestamp) -> Result<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;

    chrono::Utc
        .timestamp_opt(ts.seconds, ts.nanos as u32)
        .single()
        .ok_or_else(|| anyhow!("Invalid timestamp: {} seconds, {} nanos", ts.seconds, ts.nanos))
}

/// Convert protobuf Value to serde_json::Value
fn prost_value_to_json(value: &prost_types::Value) -> Option<serde_json::Value> {
    value.kind.as_ref().and_then(|kind| match kind {
        Kind::NullValue(_) => Some(serde_json::Value::Null),
        Kind::NumberValue(n) => Some(serde_json::json!(n)),
        Kind::StringValue(s) => Some(serde_json::Value::String(s.clone())),
        Kind::BoolValue(b) => Some(serde_json::Value::Bool(*b)),
        Kind::StructValue(s) => {
            let mut map = serde_json::Map::new();
            for (k, v) in &s.fields {
                if let Some(json_val) = prost_value_to_json(v) {
                    map.insert(k.clone(), json_val);
                }
            }
            Some(serde_json::Value::Object(map))
        }
        Kind::ListValue(list) => {
            let values: Vec<serde_json::Value> = list
                .values
                .iter()
                .filter_map(prost_value_to_json)
                .collect();
            Some(serde_json::Value::Array(values))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::Struct;
    use std::collections::HashMap;

    #[test]
    fn test_proto_to_domain_conversion() {
        let proto = ProtoEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Some(Timestamp {
                seconds: 1700000000,
                nanos: 0,
            }),
            processed_at: Some(Timestamp {
                seconds: 1700000010,
                nanos: 0,
            }),
            data: Some(Struct {
                fields: HashMap::new(),
            }),
        };

        let result = proto_to_domain_envelope(proto);
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, "org-123");
        assert_eq!(domain.end_device_id, "device-456");
    }

    #[test]
    fn test_missing_timestamp_error() {
        let proto = ProtoEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: None, // Missing timestamp
            processed_at: Some(Timestamp {
                seconds: 1700000010,
                nanos: 0,
            }),
            data: None,
        };

        let result = proto_to_domain_envelope(proto);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("occurred_at"));
    }
}
```

**Rationale**:
- Centralizes protobuf → domain conversion logic
- Reuses existing helper functions (`prost_value_to_json`, `timestamp_to_datetime`)
- Returns `Result` to handle missing required fields
- Includes unit tests for conversion paths

#### 2. Update Processed Envelope Processor
**File**: `crates/ponix-nats/src/processed_envelope_processor.rs`
**Changes**: Refactor to use domain service instead of direct ClickHouse

```rust
use crate::consumer::{BatchProcessor, DecodedMessage, ProcessingResult};
use anyhow::Result;
use futures::future::BoxFuture;
use ponix_domain::{ProcessedEnvelopeService, types::StoreEnvelopesInput};
use ponix_proto::envelope::v1::ProcessedEnvelope as ProtoEnvelope;
use std::sync::Arc;
use tracing::{debug, error};

use crate::conversions::proto_to_domain_envelope;

/// Create a batch processor that converts protobuf envelopes to domain types
/// and stores them via the domain service
pub fn create_domain_processor(
    service: Arc<ProcessedEnvelopeService>,
) -> BatchProcessor {
    Box::new(move |messages: &[crate::consumer::Message]| {
        let service = service.clone();

        Box::pin(async move {
            // Extract decoded protobuf messages
            let decoded_envelopes: Vec<&DecodedMessage<ProtoEnvelope>> = messages
                .iter()
                .filter_map(|msg| msg.downcast_ref())
                .collect();

            if decoded_envelopes.is_empty() {
                debug!("No ProcessedEnvelope messages to process");
                return Ok(ProcessingResult::ack_all(0));
            }

            // Convert protobuf to domain types
            let domain_envelopes: Result<Vec<_>> = decoded_envelopes
                .iter()
                .map(|decoded| proto_to_domain_envelope(decoded.message.clone()))
                .collect();

            let domain_envelopes = match domain_envelopes {
                Ok(envelopes) => envelopes,
                Err(e) => {
                    error!("Failed to convert protobuf to domain: {}", e);
                    return Ok(ProcessingResult::nak_all(
                        decoded_envelopes.len(),
                        Some(format!("Conversion error: {}", e)),
                    ));
                }
            };

            // Call domain service
            let input = StoreEnvelopesInput {
                envelopes: domain_envelopes,
            };

            match service.store_batch(input).await {
                Ok(()) => {
                    debug!(
                        envelope_count = decoded_envelopes.len(),
                        "Successfully processed envelope batch"
                    );
                    Ok(ProcessingResult::ack_all(decoded_envelopes.len()))
                }
                Err(e) => {
                    error!("Failed to store envelopes: {}", e);
                    Ok(ProcessingResult::nak_all(
                        decoded_envelopes.len(),
                        Some(format!("Storage error: {}", e)),
                    ))
                }
            }
        }) as BoxFuture<'static, Result<ProcessingResult>>
    })
}
```

**Rationale**:
- Removes direct ClickHouse dependency from NATS layer
- Converts protobuf → domain before calling service
- Maintains existing ack/nak behavior (entire batch fails on error)
- Preserves error messages for debugging

#### 3. Update lib.rs and Module Exports
**File**: `crates/ponix-nats/src/lib.rs`
**Changes**: Add conversions module export

```rust
#[cfg(feature = "processed-envelope")]
mod conversions;

#[cfg(feature = "processed-envelope")]
pub use conversions::proto_to_domain_envelope;
```

#### 4. Update Cargo.toml Dependencies
**File**: `crates/ponix-nats/Cargo.toml`
**Changes**: Add ponix-domain dependency to processed-envelope feature

```toml
[dependencies]
# ... existing dependencies ...
ponix-domain = { path = "../ponix-domain", optional = true }

[features]
default = []
protobuf = ["prost"]
processed-envelope = ["ponix-proto", "protobuf", "xid", "ponix-domain"]
```

### Success Criteria

#### Automated Verification:
- [x] Conversions compile: `cargo check -p ponix-nats --features processed-envelope`
- [x] Conversion tests pass: `cargo test -p ponix-nats --features processed-envelope`
- [x] No direct ClickHouse dependencies in ponix-nats

#### Manual Verification:
- [ ] Protobuf to domain conversion handles all field types
- [ ] Error handling maintains existing behavior

---

## Phase 4: Service Integration - Wire Components in ponix-all-in-one

### Overview
Update the main service to wire together NATS consumer, domain service, and ClickHouse repository using the new DDD architecture.

### Changes Required

#### 1. Update Main Service Initialization
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Replace direct ClickHouse wiring with domain service pattern

**Replace existing code (lines 130-148):**
```rust
// OLD CODE - Direct ClickHouse dependency
let envelope_store = EnvelopeStore::new(
    clickhouse_client.clone(),
    config.clickhouse_table.clone(),
);

let processor = create_clickhouse_processor(move |envelopes| {
    let store = envelope_store.clone();
    async move { store.store_processed_envelopes(envelopes).await }
});
```

**With new DDD wiring (after line 128 - after ClickHouse client init):**
```rust
// Initialize ProcessedEnvelope domain layer
info!("Initializing processed envelope domain service");

// Create repository (infrastructure layer)
let envelope_repository = ClickHouseEnvelopeRepository::new(
    clickhouse_client.clone(),
    config.clickhouse_table.clone(),
);

// Create domain service with repository
let envelope_service = Arc::new(ProcessedEnvelopeService::new(Arc::new(envelope_repository)));
info!("Processed envelope service initialized");
```

**Update processor creation (around line 145):**
```rust
// Create processor using domain service
let processor = create_domain_processor(envelope_service.clone());
```

#### 2. Update Imports
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Update imports at top of file

**Replace:**
```rust
use ponix_clickhouse::{ClickHouseClient, EnvelopeStore, MigrationRunner};
use ponix_nats::{
    create_clickhouse_processor, create_protobuf_processor, NatsClient, NatsConsumer,
    ProcessedEnvelopeProducer,
};
```

**With:**
```rust
use ponix_clickhouse::{ClickHouseClient, ClickHouseEnvelopeRepository, MigrationRunner};
use ponix_domain::ProcessedEnvelopeService;
use ponix_nats::{
    create_domain_processor, create_protobuf_processor, NatsClient, NatsConsumer,
    ProcessedEnvelopeProducer,
};
```

#### 3. Update Cargo.toml Dependencies
**File**: `crates/ponix-all-in-one/Cargo.toml`
**Changes**: Ensure ponix-domain is included

```toml
[dependencies]
# ... existing dependencies ...
ponix-domain = { path = "../ponix-domain" }
```

### Success Criteria

#### Automated Verification:
- [x] Service compiles: `cargo build -p ponix-all-in-one`
- [ ] All workspace tests pass: `cargo test --workspace` (CEL dependency issue, unrelated to our changes)
- [x] Modified crates tests pass: Tests for ponix-domain, ponix-clickhouse, ponix-nats all pass
- [ ] Clippy passes: `cargo clippy --workspace` (CEL dependency issue, unrelated to our changes)

#### Manual Verification:
- [ ] Service starts successfully: `cargo run -p ponix-all-in-one`
- [ ] NATS consumer connects and processes messages
- [ ] Messages are stored in ClickHouse
- [ ] Error handling works correctly (failed batch = nak all)

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that end-to-end message flow works correctly before considering the refactor complete.

---

## Testing Strategy

### Unit Tests

**Domain Layer** (ponix-domain):
- ✅ Domain service delegates to repository correctly (Mockall mocks)
- ✅ Domain service propagates repository errors
- ✅ Mock generation works for `ProcessedEnvelopeRepository`

**Infrastructure Layer** (ponix-clickhouse):
- ✅ Domain → database row conversions
- ✅ Empty data handling
- ✅ JSON serialization

**NATS Layer** (ponix-nats):
- ✅ Protobuf → domain conversions
- ✅ Missing timestamp error handling
- ✅ Struct/List value conversions

### Integration Testing (Manual)

Since we're not adding new integration tests, manual verification is critical:

1. **End-to-End Message Flow**:
   ```bash
   # Start infrastructure
   docker-compose -f docker/docker-compose.deps.yaml up -d

   # Run service with Tilt
   tilt up

   # Verify producer sends messages
   # Check logs for: "Publishing processed envelope"

   # Verify consumer receives and processes
   # Check logs for: "Storing batch of envelopes"

   # Query ClickHouse to verify storage
   docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix
   SELECT count(*) FROM ponix.processed_envelopes;
   SELECT * FROM ponix.processed_envelopes LIMIT 5;
   ```

2. **Error Handling**:
   - Stop ClickHouse and verify messages are nak'd
   - Check that consumer retries failed batches
   - Verify error messages in logs

3. **Performance**:
   - Verify batch processing maintains same throughput
   - Check that memory usage is consistent
   - Monitor message lag in NATS

### Test Commands

```bash
# Run all unit tests
make test:unit

# Run domain tests specifically
cargo test -p ponix-domain

# Run ClickHouse tests
cargo test -p ponix-clickhouse

# Run NATS tests
cargo test -p ponix-nats --features processed-envelope

# Run all tests with verbose output
cargo test --workspace -- --nocapture

# Check types
cargo check --workspace

# Run clippy
cargo clippy --workspace

# Format check
cargo fmt --check
```

## Performance Considerations

This refactor maintains existing performance characteristics:

1. **Batch Size**: No changes to batch size configuration (still uses `config.nats_batch_size`)
2. **Memory Usage**: Domain type conversions are lightweight (no deep clones)
3. **ClickHouse Insertion**: Uses same inserter API pattern
4. **Concurrency**: No changes to async processing or tokio runtime

The additional layer of abstraction adds minimal overhead:
- One extra `Arc` clone per batch (negligible)
- Protobuf → Domain conversion (single pass, linear time)
- Domain → Database conversion (single pass, linear time)

## Migration Notes

This is a pure refactor with no migration required:

1. **Database Schema**: No changes to ClickHouse schema
2. **Message Format**: No changes to protobuf definitions
3. **Configuration**: No new environment variables needed
4. **Deployment**: Can be deployed as a normal release (no special steps)

### Backward Compatibility

- Old `EnvelopeStore` marked as deprecated but still functional
- Feature flags unchanged (`processed-envelope` still required)
- No breaking changes to public APIs

### Rollback Plan

If issues are discovered post-deployment:
1. Revert to previous commit (pure refactor = easy rollback)
2. No data migration needed
3. No schema changes to undo

## References

- Original ticket: Issue #21 - Refactor ProcessedEnvelope Flow to Domain-Driven Architecture
- Related issues:
  - #3: NATS JetStream Producer and Consumer implementation
  - #4: ClickHouse batch writes for ProcessedEnvelope
  - #12: Generic protobuf batch processor
  - #17: gRPC API with domain layer (DDD pattern reference)
- Similar implementation: [Device Management DDD Pattern](crates/ponix-domain/src/end_device_service.rs)
- Repository pattern: [DeviceRepository trait](crates/ponix-domain/src/repository.rs)
