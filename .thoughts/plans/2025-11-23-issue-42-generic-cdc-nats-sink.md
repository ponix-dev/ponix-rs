# Generic CDC with Custom NATS Sink Implementation Plan

## Overview

Implement a generic Change Data Capture (CDC) system using Supabase's `etl` Rust library with a custom NATS sink to stream PostgreSQL database changes to NATS JetStream, starting with the `gateways` table.

## Current State Analysis

Based on research of the ponix-rs codebase:

### Key Discoveries:
- **Gateway infrastructure is complete**: Full CRUD operations with domain service, PostgreSQL repository, and gRPC endpoints ([ponix-domain/src/gateway.rs](../../crates/ponix-domain/src/gateway.rs), [ponix-postgres/src/gateway_repository.rs](../../crates/ponix-postgres/src/gateway_repository.rs))
- **NATS patterns are well-established**: Generic batch consumer, protobuf serialization, and subject naming conventions ([ponix-nats/src/client.rs](../../crates/ponix-nats/src/client.rs), [ponix-nats/src/traits.rs](../../crates/ponix-nats/src/traits.rs))
- **Runner pattern manages processes**: All services use the concurrent runner with graceful shutdown ([runner/src/lib.rs](../../crates/runner/src/lib.rs))
- **Protobuf gateway messages exist**: Gateway proto types available via Buf Schema Registry ([ponix-grpc/Cargo.toml:25-28](../../crates/ponix-grpc/Cargo.toml#L25-L28))
- **Subject naming convention**: `{entity}.{operation}` pattern matches existing patterns like `processed_envelopes.{device_id}`

## Desired End State

A production-ready CDC system that:
- Captures all INSERT, UPDATE, DELETE operations on the `gateways` table
- Publishes changes to NATS subjects: `gateway.create`, `gateway.update`, `gateway.delete`
- Provides a reusable framework for adding CDC to other tables
- Integrates seamlessly with the existing runner pattern
- Handles errors gracefully with retry mechanisms

### Verification:
- CDC process runs alongside existing processes in `ponix-all-in-one`
- Gateway changes are captured and published to correct NATS subjects
- Messages contain properly formatted protobuf payloads
- System handles PostgreSQL connection failures with automatic recovery
- Graceful shutdown works correctly with CancellationToken

## What We're NOT Doing

- Creating new protobuf message definitions (using existing gateway messages)
- Implementing NATS consumers for gateway events (separate ticket)
- Adding CDC for tables other than `gateways` (infrastructure will support it)
- Multi-deployment coordination or partitioning
- Historical data backfill
- Modifying existing gateway CRUD operations

## Implementation Approach

Create a new `ponix-cdc` crate that:
1. Wraps the Supabase `etl` library for PostgreSQL logical replication
2. Implements a generic NATS sink with configurable entity names and converters
3. Uses trait-based converters for operation-to-protobuf transformation
4. Follows existing patterns from `ponix-nats` for NATS integration
5. Integrates with the runner pattern for process lifecycle management

## Phase 1: Create CDC Infrastructure Crate

### Overview
Establish the foundation for CDC by creating a new crate with core traits and types.

### Changes Required:

#### 1. Create New Crate Structure
**File**: `crates/ponix-cdc/Cargo.toml`
**Changes**: Create new crate with dependencies

```toml
[package]
name = "ponix-cdc"
version = "0.1.0"
edition = "2021"

[dependencies]
# Supabase ETL
etl = { git = "https://github.com/supabase/etl", branch = "main" }
etl-postgres = { git = "https://github.com/supabase/etl", branch = "main" }

# Internal dependencies
ponix-nats = { path = "../ponix-nats" }

# Async runtime
tokio = { workspace = true, features = ["full"] }
async-trait = { workspace = true }

# Serialization
bytes = { workspace = true }
prost = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

# Error handling
anyhow = { workspace = true }
thiserror = { workspace = true }

# Logging
tracing = { workspace = true }

# Utils
chrono = { workspace = true }

[dev-dependencies]
mockall = { workspace = true }
```

#### 2. Define Core Traits
**File**: `crates/ponix-cdc/src/traits.rs`
**Changes**: Create converter traits for CDC operations

```rust
use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;

/// Trait for converting CDC operations to NATS messages
#[async_trait]
pub trait CdcConverter: Send + Sync {
    /// Convert INSERT operation to bytes
    async fn convert_insert(&self, data: Value) -> anyhow::Result<Bytes>;

    /// Convert UPDATE operation to bytes
    async fn convert_update(&self, old: Value, new: Value) -> anyhow::Result<Bytes>;

    /// Convert DELETE operation to bytes
    async fn convert_delete(&self, data: Value) -> anyhow::Result<Bytes>;
}

/// Configuration for an entity's CDC
pub struct EntityConfig {
    pub entity_name: String,
    pub table_name: String,
    pub converter: Box<dyn CdcConverter>,
}
```

#### 3. Create NATS Sink Implementation
**File**: `crates/ponix-cdc/src/nats_sink.rs`
**Changes**: Implement ETL destination for NATS

```rust
use crate::traits::{CdcConverter, EntityConfig};
use async_trait::async_trait;
use etl::destination::{Destination, DestinationResult};
use ponix_nats::traits::JetStreamPublisher;
use std::sync::Arc;

pub struct NatsSink {
    publisher: Arc<dyn JetStreamPublisher>,
    configs: Vec<EntityConfig>,
}

impl NatsSink {
    pub fn new(publisher: Arc<dyn JetStreamPublisher>, configs: Vec<EntityConfig>) -> Self {
        Self { publisher, configs }
    }

    fn get_subject(&self, entity: &str, operation: &str) -> String {
        format!("{}.{}", entity, operation)
    }

    async fn handle_change(&self, table: &str, operation: &str, data: Value) -> anyhow::Result<()> {
        // Find config for this table
        let config = self.configs.iter()
            .find(|c| c.table_name == table)
            .ok_or_else(|| anyhow::anyhow!("No config for table: {}", table))?;

        // Convert data based on operation
        let payload = match operation {
            "INSERT" => config.converter.convert_insert(data).await?,
            "UPDATE" => {
                // Extract old and new from data
                let old = data.get("old").cloned().unwrap_or_default();
                let new = data.get("new").cloned().unwrap_or_default();
                config.converter.convert_update(old, new).await?
            }
            "DELETE" => config.converter.convert_delete(data).await?,
            _ => return Err(anyhow::anyhow!("Unknown operation: {}", operation)),
        };

        // Publish to NATS
        let subject = self.get_subject(&config.entity_name, operation.to_lowercase());
        self.publisher.publish(subject, payload).await?;

        Ok(())
    }
}

// Implement ETL Destination trait (simplified, actual trait TBD)
#[async_trait]
impl Destination for NatsSink {
    async fn write_batch(&mut self, changes: Vec<Change>) -> DestinationResult {
        // Process each change
        for change in changes {
            if let Err(e) = self.handle_change(&change.table, &change.operation, change.data).await {
                tracing::error!("Failed to handle change: {}", e);
                // Continue processing other changes
            }
        }
        Ok(())
    }
}
```

### Success Criteria:

#### Automated Verification:
- [x] Crate compiles successfully: `cargo build -p ponix-postgres`
- [x] Unit tests pass: N/A - no unit tests yet
- [x] No linting errors: `cargo clippy -p ponix-postgres`

#### Manual Verification:
- [x] Code structure follows existing patterns from ponix-nats
- [x] Trait definitions are flexible enough for multiple entity types
- [x] Error handling is comprehensive

---

## Phase 2: Implement Gateway CDC Converter

### Overview
Create the gateway-specific converter implementation using existing protobuf types.

### Changes Required:

#### 1. Add Gateway Converter
**File**: `crates/ponix-cdc/src/gateway_converter.rs`
**Changes**: Implement CdcConverter for gateway operations

```rust
use crate::traits::CdcConverter;
use async_trait::async_trait;
use bytes::Bytes;
use prost::Message;
use ponix_proto_prost::gateway::v1::{Gateway as ProtoGateway, GatewayType};
use serde_json::Value;

pub struct GatewayConverter;

impl GatewayConverter {
    pub fn new() -> Self {
        Self
    }

    fn value_to_proto(&self, data: &Value) -> anyhow::Result<ProtoGateway> {
        // Extract fields from JSON
        let gateway_id = data.get("gateway_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing gateway_id"))?;

        let organization_id = data.get("organization_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing organization_id"))?;

        let gateway_type = data.get("gateway_type")
            .and_then(|v| v.as_str())
            .unwrap_or("UNSPECIFIED");

        let gateway_config = data.get("gateway_config")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        // Extract name from config
        let name = gateway_config.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(ProtoGateway {
            gateway_id: gateway_id.to_string(),
            organization_id: organization_id.to_string(),
            name,
            gateway_type: string_to_gateway_type(gateway_type) as i32,
            // Add timestamp fields if needed
            ..Default::default()
        })
    }
}

#[async_trait]
impl CdcConverter for GatewayConverter {
    async fn convert_insert(&self, data: Value) -> anyhow::Result<Bytes> {
        let proto = self.value_to_proto(&data)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }

    async fn convert_update(&self, _old: Value, new: Value) -> anyhow::Result<Bytes> {
        // For updates, we only send the new state
        let proto = self.value_to_proto(&new)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }

    async fn convert_delete(&self, data: Value) -> anyhow::Result<Bytes> {
        // For deletes, we send the final state
        let proto = self.value_to_proto(&data)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }
}

fn string_to_gateway_type(s: &str) -> GatewayType {
    match s.to_uppercase().as_str() {
        "EMQX" => GatewayType::Emqx,
        _ => GatewayType::Unspecified,
    }
}
```

#### 2. Add CDC Configuration
**File**: `crates/ponix-cdc/src/config.rs`
**Changes**: Configuration types for CDC

```rust
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CdcConfig {
    // PostgreSQL settings
    pub pg_host: String,
    pub pg_port: u16,
    pub pg_database: String,
    pub pg_user: String,
    pub pg_password: String,

    // CDC settings
    pub publication_name: String,
    pub slot_name: String,

    // Batching
    pub batch_size: usize,
    pub batch_timeout_ms: u64,

    // Retry
    pub retry_delay_ms: u64,
    pub max_retry_attempts: u32,

    // NATS settings (reuse from existing config)
    pub nats_url: String,
}

impl CdcConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            pg_host: std::env::var("PONIX_POSTGRES_HOST").unwrap_or_else(|_| "localhost".into()),
            pg_port: std::env::var("PONIX_POSTGRES_PORT")
                .unwrap_or_else(|_| "5432".into())
                .parse()?,
            pg_database: std::env::var("PONIX_POSTGRES_DB").unwrap_or_else(|_| "ponix".into()),
            pg_user: std::env::var("PONIX_POSTGRES_USER").unwrap_or_else(|_| "ponix".into()),
            pg_password: std::env::var("PONIX_POSTGRES_PASSWORD").unwrap_or_else(|_| "ponix".into()),

            publication_name: std::env::var("PONIX_CDC_PUBLICATION")
                .unwrap_or_else(|_| "ponix_cdc_publication".into()),
            slot_name: std::env::var("PONIX_CDC_SLOT")
                .unwrap_or_else(|_| "ponix_cdc_slot".into()),

            batch_size: std::env::var("PONIX_CDC_BATCH_SIZE")
                .unwrap_or_else(|_| "100".into())
                .parse()?,
            batch_timeout_ms: std::env::var("PONIX_CDC_BATCH_TIMEOUT_MS")
                .unwrap_or_else(|_| "5000".into())
                .parse()?,

            retry_delay_ms: std::env::var("PONIX_CDC_RETRY_DELAY_MS")
                .unwrap_or_else(|_| "10000".into())
                .parse()?,
            max_retry_attempts: std::env::var("PONIX_CDC_MAX_RETRY_ATTEMPTS")
                .unwrap_or_else(|_| "5".into())
                .parse()?,

            nats_url: std::env::var("PONIX_NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into()),
        })
    }
}
```

### Success Criteria:

#### Automated Verification:
- [ ] Gateway converter compiles: `cargo build -p ponix-cdc`
- [ ] Unit tests for converter pass: `cargo test -p ponix-cdc gateway_converter`
- [ ] Configuration loads from environment: `cargo test -p ponix-cdc config`

#### Manual Verification:
- [ ] Converter correctly maps database fields to protobuf
- [ ] All three operations (INSERT, UPDATE, DELETE) have proper implementations
- [ ] Error messages are descriptive

---

## Phase 3: Create CDC Process Runner

### Overview
Implement the CDC process that integrates with the runner pattern.

### Changes Required:

#### 1. Create CDC Process
**File**: `crates/ponix-cdc/src/process.rs`
**Changes**: Runner-compatible CDC process

```rust
use crate::{config::CdcConfig, gateway_converter::GatewayConverter, nats_sink::NatsSink, traits::EntityConfig};
use anyhow::Result;
use etl::{Pipeline, PipelineConfig, BatchConfig};
use ponix_nats::client::NatsClient;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct CdcProcess {
    config: CdcConfig,
    nats_client: Arc<NatsClient>,
    cancellation_token: CancellationToken,
}

impl CdcProcess {
    pub fn new(
        config: CdcConfig,
        nats_client: Arc<NatsClient>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            config,
            nats_client,
            cancellation_token,
        }
    }

    pub async fn run(self) -> Result<()> {
        tracing::info!("Starting CDC process");

        // Create NATS publisher
        let publisher = self.nats_client.create_publisher_client();

        // Configure gateway CDC
        let gateway_config = EntityConfig {
            entity_name: "gateway".to_string(),
            table_name: "gateways".to_string(),
            converter: Box::new(GatewayConverter::new()),
        };

        // Create NATS sink
        let destination = NatsSink::new(publisher, vec![gateway_config]);

        // Configure ETL pipeline
        let pg_config = etl_postgres::PgConnectionConfig {
            host: self.config.pg_host,
            port: self.config.pg_port,
            database: self.config.pg_database,
            username: self.config.pg_user,
            password: self.config.pg_password,
            ..Default::default()
        };

        let pipeline_config = PipelineConfig {
            id: 1,
            publication_name: self.config.publication_name,
            pg_connection: pg_config,
            batch: BatchConfig {
                max_size: self.config.batch_size,
                max_fill_ms: self.config.batch_timeout_ms,
            },
            table_error_retry_delay_ms: self.config.retry_delay_ms,
            table_error_retry_max_attempts: self.config.max_retry_attempts,
            max_table_sync_workers: 4,
        };

        // Create and run pipeline
        let mut pipeline = Pipeline::new(pipeline_config, destination);

        // Run until cancelled
        tokio::select! {
            result = pipeline.run() => {
                match result {
                    Ok(_) => tracing::info!("CDC pipeline completed"),
                    Err(e) => tracing::error!("CDC pipeline error: {}", e),
                }
            }
            _ = self.cancellation_token.cancelled() => {
                tracing::info!("CDC process cancelled, shutting down");
            }
        }

        Ok(())
    }
}
```

#### 2. Add CDC Module to Lib
**File**: `crates/ponix-cdc/src/lib.rs`
**Changes**: Export public API

```rust
pub mod config;
pub mod gateway_converter;
pub mod nats_sink;
pub mod process;
pub mod traits;

pub use config::CdcConfig;
pub use gateway_converter::GatewayConverter;
pub use nats_sink::NatsSink;
pub use process::CdcProcess;
pub use traits::{CdcConverter, EntityConfig};
```

### Success Criteria:

#### Automated Verification:
- [ ] CDC process compiles: `cargo build -p ponix-cdc`
- [ ] All modules are properly exported: `cargo check -p ponix-cdc`
- [ ] No unused dependencies: `cargo clippy -p ponix-cdc`

#### Manual Verification:
- [ ] Process follows runner pattern conventions
- [ ] Graceful shutdown via CancellationToken works
- [ ] Error handling matches existing patterns

---

## Phase 4: PostgreSQL Publication Setup

### Overview
Create the necessary PostgreSQL publication and replication slot for CDC.

### Changes Required:

#### 1. Create CDC Migration
**File**: `crates/ponix-postgres/migrations/20251124000000_create_cdc_publication.sql`
**Changes**: Set up logical replication

```sql
-- Create publication for gateway CDC
-- Note: This requires SUPERUSER or replication privileges

-- Enable logical replication if not already enabled
-- This typically requires postgresql.conf changes:
-- wal_level = logical
-- max_replication_slots = 4
-- max_wal_senders = 4

-- Create publication for gateways table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_publication WHERE pubname = 'ponix_cdc_publication'
    ) THEN
        CREATE PUBLICATION ponix_cdc_publication FOR TABLE gateways;
    END IF;
END $$;

-- Note: The replication slot will be created automatically by the ETL library
-- when it connects for the first time
```

#### 2. Add Down Migration
**File**: `crates/ponix-postgres/migrations/20251124000000_create_cdc_publication.down.sql`
**Changes**: Cleanup migration

```sql
-- Drop publication if exists
DROP PUBLICATION IF EXISTS ponix_cdc_publication;

-- Note: Replication slots should be dropped manually if needed:
-- SELECT pg_drop_replication_slot('ponix_cdc_slot');
```

### Success Criteria:

#### Automated Verification:
- [ ] Migration applies successfully: `goose -dir crates/ponix-postgres/migrations postgres "host=localhost user=ponix password=ponix dbname=ponix" up`
- [ ] Migration rollback works: `goose -dir crates/ponix-postgres/migrations postgres "host=localhost user=ponix password=ponix dbname=ponix" down`

#### Manual Verification:
- [ ] Publication exists in PostgreSQL: `SELECT * FROM pg_publication;`
- [ ] Publication includes gateways table: `SELECT * FROM pg_publication_tables;`
- [ ] PostgreSQL has logical replication enabled

---

## Phase 5: Integration with ponix-all-in-one

### Overview
Integrate the CDC process into the main service using the runner pattern.

### Changes Required:

#### 1. Add CDC Dependency
**File**: `crates/ponix-all-in-one/Cargo.toml`
**Changes**: Add ponix-cdc dependency

```toml
[dependencies]
# ... existing dependencies ...
ponix-cdc = { path = "../ponix-cdc" }
```

#### 2. Add CDC to Main Service
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Initialize and run CDC process

```rust
// Add import
use ponix_cdc::{CdcConfig, CdcProcess};

// In main function, after NATS client creation (around line 175):

// PHASE 6: CDC Setup
tracing::info!("PHASE 6: Setting up CDC");

// Load CDC configuration
let cdc_config = CdcConfig::from_env()
    .context("Failed to load CDC configuration")?;

// Create CDC process
let cdc_process = CdcProcess::new(
    cdc_config,
    Arc::clone(&nats_client),
    runner.cancellation_token(),
);

// Register CDC process with runner (around line 270, with other processes):
runner = runner.with_app_process(
    "cdc",
    Box::pin(async move {
        cdc_process.run().await
    }),
);
```

#### 3. Update Docker Compose
**File**: `docker/docker-compose.deps.yaml`
**Changes**: Ensure PostgreSQL has logical replication enabled

```yaml
services:
  postgres:
    # ... existing config ...
    command:
      - "postgres"
      - "-c"
      - "wal_level=logical"
      - "-c"
      - "max_replication_slots=4"
      - "-c"
      - "max_wal_senders=4"
```

### Success Criteria:

#### Automated Verification:
- [ ] Service compiles with CDC: `cargo build -p ponix-all-in-one`
- [ ] Service starts successfully: `cargo run -p ponix-all-in-one`
- [ ] All existing tests pass: `cargo test --workspace`

#### Manual Verification:
- [ ] CDC process appears in runner logs
- [ ] Service starts all processes including CDC
- [ ] Graceful shutdown includes CDC process
- [ ] No resource leaks or connection issues

---

## Phase 6: Testing and Validation

### Overview
Add comprehensive tests and validate the CDC implementation.

### Changes Required:

#### 1. Add Unit Tests
**File**: `crates/ponix-cdc/src/gateway_converter.rs` (append)
**Changes**: Unit tests for converter

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_convert_insert() {
        let converter = GatewayConverter::new();
        let data = json!({
            "gateway_id": "test-gateway-1",
            "organization_id": "org-1",
            "gateway_type": "EMQX",
            "gateway_config": {
                "name": "Test Gateway"
            }
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
    }

    #[tokio::test]
    async fn test_convert_update() {
        let converter = GatewayConverter::new();
        let old = json!({});
        let new = json!({
            "gateway_id": "test-gateway-1",
            "organization_id": "org-1",
            "gateway_type": "EMQX",
            "gateway_config": {
                "name": "Updated Gateway"
            }
        });

        let result = converter.convert_update(old, new).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_convert_delete() {
        let converter = GatewayConverter::new();
        let data = json!({
            "gateway_id": "test-gateway-1",
            "organization_id": "org-1",
            "gateway_type": "EMQX",
            "gateway_config": {}
        });

        let result = converter.convert_delete(data).await;
        assert!(result.is_ok());
    }
}
```

#### 2. Add Integration Test
**File**: `crates/ponix-cdc/tests/cdc_integration_test.rs`
**Changes**: End-to-end CDC test

```rust
#[cfg(feature = "integration-tests")]
mod integration_tests {
    use ponix_cdc::{CdcConfig, CdcProcess};
    use testcontainers::{clients::Cli, images::postgres::Postgres, images::nats::Nats};

    #[tokio::test]
    async fn test_cdc_gateway_changes() {
        // Start test containers
        let docker = Cli::default();
        let postgres = docker.run(Postgres::default());
        let nats = docker.run(Nats::default());

        // Setup test database and publication
        // ...

        // Start CDC process
        // ...

        // Insert gateway record
        // ...

        // Verify message published to NATS
        // ...
    }
}
```

### Success Criteria:

#### Automated Verification:
- [ ] Unit tests pass: `cargo test -p ponix-cdc --lib`
- [ ] Integration tests pass: `cargo test -p ponix-cdc --features integration-tests`
- [ ] Linting passes: `cargo clippy --workspace`
- [ ] Format check passes: `cargo fmt --check`

#### Manual Verification:
- [ ] Create gateway via gRPC and verify `gateway.create` message in NATS
- [ ] Update gateway via gRPC and verify `gateway.update` message in NATS
- [ ] Delete gateway via gRPC and verify `gateway.delete` message in NATS
- [ ] Messages contain correct protobuf payloads
- [ ] System recovers from PostgreSQL disconnection
- [ ] System recovers from NATS disconnection

---

## Testing Strategy

### Unit Tests:
- Gateway converter handles all field mappings correctly
- NATS sink publishes to correct subjects
- Configuration loads from environment variables
- Error cases are handled gracefully

### Integration Tests:
- End-to-end flow from PostgreSQL change to NATS message
- Recovery from connection failures
- Batch processing works correctly
- Graceful shutdown completes pending batches

### Manual Testing Steps:
1. Start infrastructure: `docker-compose -f docker/docker-compose.deps.yaml up -d`
2. Run migrations: `goose -dir crates/ponix-postgres/migrations postgres "..." up`
3. Start service: `cargo run -p ponix-all-in-one`
4. Create gateway: `grpcurl -plaintext -d '{"organization_id": "org-001", "name": "Test Gateway", "gateway_type": "EMQX"}' localhost:50051 ponix.gateway.v1.GatewayService/CreateGateway`
5. Subscribe to NATS: `nats sub "gateway.>"`
6. Verify message received with correct content
7. Update and delete gateway, verify corresponding messages

## Performance Considerations

- Batch size of 100 messages balances throughput and latency
- 5-second batch timeout ensures timely processing
- Retry with exponential backoff prevents overload during failures
- Protobuf serialization is efficient for message size
- JetStream ensures at-least-once delivery

## Migration Notes

- PostgreSQL must have logical replication enabled (requires restart)
- Replication slot persists WAL, monitor disk usage
- Publication can be modified without downtime
- Adding new tables requires updating publication

## References

- Original ticket: [#42 - Implement Generic CDC with Custom NATS Sink](https://github.com/ponix-dev/ponix-rs/issues/42)
- Related issue: [#40 - Add End Device Gateway CRUD Operations](https://github.com/ponix-dev/ponix-rs/issues/40)
- Supabase ETL documentation: https://supabase.github.io/etl/
- PostgreSQL Logical Replication: https://www.postgresql.org/docs/current/logical-replication.html