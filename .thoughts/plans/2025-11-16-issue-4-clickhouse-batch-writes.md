# ClickHouse Batch Writes for ProcessedEnvelope Implementation Plan

## Overview

Implement ClickHouse integration for batch writing `ProcessedEnvelope` messages consumed from NATS JetStream. This replaces the current logging-only processor with actual ClickHouse persistence, applying lessons learned from previous integration test findings.

## Current State Analysis

### What Exists
- NATS consumer implementation (issue #3) is complete and functional
- ProcessedEnvelope messages are consumed from NATS in batches and currently only logged
- Runner-based lifecycle management is in place
- Environment-variable based configuration system
- Docker multi-stage build setup with BSR authentication

### What's Missing
- ponix-clickhouse crate (was removed after integration testing revealed issues)
- ClickHouse database schema and migrations
- ClickHouse batch write implementation
- ClickHouse service in docker-compose.deps.yaml
- Goose binary in Docker container

### Key Discoveries from Integration Tests

**Root Cause Identified**: The previous implementation failed because DateTime fields were missing required serde attributes when using JSON type alongside DateTime columns.

**The Solution**:
1. Add `#[serde(with = "clickhouse::serde::chrono::datetime")]` to all DateTime fields
2. Use clickhouse-rs 0.14 with proper JSON type settings
3. Enable both `allow_experimental_json_type=1` and the JSON binary format settings

**Official Examples to Follow**:
- [DateTime with Row Derive](https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_derive_simple.rs) - Shows proper serde attributes for DateTime
- [JSON Type Usage](https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_new_json.rs) - Shows required settings for JSON type

## Desired End State

A working ClickHouse integration where:
- ProcessedEnvelope messages are batch-written to ClickHouse
- Migrations run automatically on startup
- JSON and DateTime types work together correctly with proper serde attributes
- Local development works with Docker Compose
- Integration tests verify the complete flow

### Verification
Run the all-in-one service with ClickHouse and verify:
1. Migrations execute successfully on startup
2. NATS messages are consumed and written to ClickHouse
3. Query ClickHouse to see persisted ProcessedEnvelope records
4. Integration tests pass

## What We're NOT Doing

- Query functionality (histogram queries, etc.) - future issue
- Advanced ClickHouse optimizations beyond basic MergeTree setup
- Multi-database support (PostgreSQL, etc.)
- Custom migration tooling (using Goose CLI as-is)

## Implementation Approach

Create a new `ponix-clickhouse` crate following the lessons learned from integration testing. The key insight is combining patterns from two official examples:
1. Use DateTime serde attributes from the DateTime example
2. Use JSON type settings from the JSON example
3. Apply both patterns together in our schema

Integrate with the existing NATS consumer by modifying the batch processor to call ClickHouse instead of just logging.

---

## Phase 1: Create ponix-clickhouse Crate Foundation

### Overview
Set up the basic crate structure, dependencies, and ClickHouse client with proper configuration based on official examples.

### Changes Required

#### 1. Add ponix-clickhouse to workspace
**File**: `Cargo.toml`
**Changes**: Add ponix-clickhouse to workspace members

```toml
[workspace]
members = [
    "crates/runner",
    "crates/ponix-nats",
    "crates/ponix-clickhouse",  # Add this
    "crates/ponix-all-in-one",
]
```

#### 2. Create ponix-clickhouse crate
**File**: `crates/ponix-clickhouse/Cargo.toml`
**Changes**: Create new Cargo.toml with clickhouse-rs 0.14

```toml
[package]
name = "ponix-clickhouse"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
# ClickHouse client with all necessary features
clickhouse = { version = "0.14", features = ["lz4", "inserter", "watch"] }
tokio = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
# CRITICAL: Use chrono with serde feature for DateTime serde attributes
chrono = { version = "0.4", features = ["serde"] }

# Protobuf dependencies for ProcessedEnvelope
ponix-proto = { package = "ponix_ponix_community_neoeinstein-prost", version = "0.4.0-20251105022230-76aad410c133.1", registry = "buf" }
prost-types = { workspace = true }

[dev-dependencies]
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["clickhouse"] }
tokio-test = "0.4"
```

#### 3. Create crate structure
**Files to create**:
- `crates/ponix-clickhouse/src/lib.rs` - Public API exports
- `crates/ponix-clickhouse/src/client.rs` - ClickHouse client wrapper
- `crates/ponix-clickhouse/src/config.rs` - Configuration structure
- `crates/ponix-clickhouse/src/envelope_store.rs` - ProcessedEnvelope storage
- `crates/ponix-clickhouse/src/migration.rs` - Migration runner
- `crates/ponix-clickhouse/src/models.rs` - ClickHouse row models

#### 4. Implement ClickHouse client with JSON support
**File**: `crates/ponix-clickhouse/src/client.rs`
**Changes**: Create client with all required settings from JSON example

```rust
use anyhow::Result;
use clickhouse::Client;

#[derive(Clone)]
pub struct ClickHouseClient {
    client: Client,
}

impl ClickHouseClient {
    pub fn new(url: &str, database: &str, username: &str, password: &str) -> Self {
        let client = Client::default()
            .with_url(url)
            .with_database(database)
            .with_user(username)
            .with_password(password)
            .with_compression(clickhouse::Compression::Lz4)
            // CRITICAL: All three settings needed for JSON type (from official example)
            .with_option("allow_experimental_json_type", "1")
            .with_option("input_format_binary_read_json_as_string", "1")
            .with_option("output_format_binary_write_json_as_string", "1");

        Self { client }
    }

    pub async fn ping(&self) -> Result<()> {
        self.client.query("SELECT 1").fetch_one::<u8>().await?;
        Ok(())
    }

    pub fn get_client(&self) -> &Client {
        &self.client
    }
}
```

#### 5. Create configuration structure
**File**: `crates/ponix-clickhouse/src/config.rs`
**Changes**: Define ClickHouse configuration

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseConfig {
    pub url: String,
    pub database: String,
    pub username: String,
    pub password: String,
    pub migrations_dir: String,
    pub goose_binary_path: String,
}

impl Default for ClickHouseConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8123".to_string(),
            database: "ponix".to_string(),
            username: "default".to_string(),
            password: "".to_string(),
            migrations_dir: "crates/ponix-clickhouse/migrations".to_string(),
            goose_binary_path: "goose".to_string(),
        }
    }
}
```

#### 6. Create ProcessedEnvelopeRow with proper serde attributes
**File**: `crates/ponix-clickhouse/src/models.rs`
**Changes**: Define row model with CRITICAL DateTime serde attributes from official example

```rust
use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ProcessedEnvelopeRow {
    pub organization_id: String,
    pub end_device_id: String,
    // CRITICAL: This serde attribute is required when using JSON type alongside DateTime
    // Reference: https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_derive_simple.rs
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub occurred_at: DateTime<Utc>,
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub processed_at: DateTime<Utc>,
    // JSON type in ClickHouse, mapped to String in Rust
    pub data: String,
}
```

#### 7. Create lib.rs exports
**File**: `crates/ponix-clickhouse/src/lib.rs`
**Changes**: Export public API

```rust
mod client;
mod config;
mod envelope_store;
mod migration;
mod models;

pub use client::ClickHouseClient;
pub use config::ClickHouseConfig;
pub use envelope_store::EnvelopeStore;
pub use migration::MigrationRunner;
pub use models::ProcessedEnvelopeRow;
```

### Success Criteria

#### Automated Verification:
- [x] ponix-clickhouse crate compiles: `cargo build -p ponix-clickhouse`
- [x] No clippy warnings: `cargo clippy -p ponix-clickhouse`
- [x] Format check passes: `cargo fmt --check`
- [x] Client can be instantiated in tests

#### Manual Verification:
- [ ] Workspace structure looks correct
- [ ] Dependencies are appropriate versions

---

## Phase 2: Implement Migration System

### Overview
Create Goose migration files and a Rust wrapper that calls the Goose CLI to run migrations on startup.

### Changes Required

#### 1. Create migrations directory structure
**Directory**: `crates/ponix-clickhouse/migrations/`
**Changes**: Create directory for SQL migration files

#### 2. Port initial migration
**File**: `crates/ponix-clickhouse/migrations/20251026224743_init.sql`
**Changes**: Create initial schema with JSON type

```sql
-- +goose Up
-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS processed_envelopes (
  end_device_id String NOT NULL,
  occurred_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data JSON NOT NULL
) ENGINE = MergeTree()
PRIMARY KEY (occurred_at, end_device_id)
ORDER BY (occurred_at, end_device_id)
PARTITION BY toYYYYMM(occurred_at)
SETTINGS index_granularity = 8192;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP TABLE IF EXISTS processed_envelopes;
-- +goose StatementEnd
```

#### 3. Port organization_id migration
**File**: `crates/ponix-clickhouse/migrations/20251104110101_add_organization_id.sql`
**Changes**: Add organization_id column with data migration

```sql
-- +goose Up
-- +goose StatementBegin
-- Create new table with organization_id
CREATE TABLE IF NOT EXISTS processed_envelopes_new (
  organization_id String NOT NULL,
  end_device_id String NOT NULL,
  occurred_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data JSON NOT NULL
) ENGINE = MergeTree()
PRIMARY KEY (organization_id, occurred_at, end_device_id)
ORDER BY (organization_id, occurred_at, end_device_id)
PARTITION BY toYYYYMM(occurred_at)
SETTINGS index_granularity = 8192;
-- +goose StatementEnd

-- +goose StatementBegin
-- Migrate data from old table if it exists
INSERT INTO processed_envelopes_new
SELECT
  '' as organization_id,  -- Default empty string for existing records
  end_device_id,
  occurred_at,
  processed_at,
  data
FROM processed_envelopes;
-- +goose StatementEnd

-- +goose StatementBegin
-- Drop old table
DROP TABLE IF EXISTS processed_envelopes;
-- +goose StatementEnd

-- +goose StatementBegin
-- Rename new table to original name
RENAME TABLE processed_envelopes_new TO processed_envelopes;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
-- Create old table structure
CREATE TABLE IF NOT EXISTS processed_envelopes_old (
  end_device_id String NOT NULL,
  occurred_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data JSON NOT NULL
) ENGINE = MergeTree()
PRIMARY KEY (occurred_at, end_device_id)
ORDER BY (occurred_at, end_device_id)
PARTITION BY toYYYYMM(occurred_at)
SETTINGS index_granularity = 8192;
-- +goose StatementEnd

-- +goose StatementBegin
-- Migrate data back (losing organization_id)
INSERT INTO processed_envelopes_old
SELECT
  end_device_id,
  occurred_at,
  processed_at,
  data
FROM processed_envelopes;
-- +goose StatementEnd

-- +goose StatementBegin
DROP TABLE IF EXISTS processed_envelopes;
-- +goose StatementEnd

-- +goose StatementBegin
RENAME TABLE processed_envelopes_old TO processed_envelopes;
-- +goose StatementEnd
```

#### 4. Implement migration runner
**File**: `crates/ponix-clickhouse/src/migration.rs`
**Changes**: Create Goose CLI wrapper

```rust
use anyhow::{Context, Result};
use std::process::Command;
use tracing::info;

pub struct MigrationRunner {
    goose_binary_path: String,
    migrations_dir: String,
    clickhouse_url: String,
    database: String,
    user: String,
    password: String,
}

impl MigrationRunner {
    pub fn new(
        goose_binary_path: String,
        migrations_dir: String,
        clickhouse_url: String,
        database: String,
        user: String,
        password: String,
    ) -> Self {
        Self {
            goose_binary_path,
            migrations_dir,
            clickhouse_url,
            database,
            user,
            password,
        }
    }

    pub async fn run_migrations(&self) -> Result<()> {
        // Build connection string for goose with JSON type support
        // CRITICAL: Include allow_experimental_json_type in DSN for migrations
        let dsn = format!(
            "clickhouse://{}:{}@{}/{}?allow_experimental_json_type=1",
            self.user, self.password, self.clickhouse_url, self.database
        );

        info!("Running database migrations from {}", self.migrations_dir);

        let output = Command::new(&self.goose_binary_path)
            .args([
                "-dir", &self.migrations_dir,
                "clickhouse",
                &dsn,
                "up",
            ])
            .output()
            .context("Failed to execute goose command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!(
                "Migration failed:\nSTDOUT: {}\nSTDERR: {}",
                stdout,
                stderr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("Migrations completed successfully: {}", stdout);

        Ok(())
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] Migration files have valid SQL syntax
- [x] Migration runner compiles: `cargo build -p ponix-clickhouse`
- [x] No clippy warnings: `cargo clippy -p ponix-clickhouse`

#### Manual Verification:
- [ ] With ClickHouse running locally and goose installed, migrations execute successfully
- [ ] Verify schema in ClickHouse matches expected structure
- [ ] Running migrations twice (idempotent) succeeds without errors

**Implementation Note**: After completing automated verification, pause for manual confirmation that local migration testing was successful before proceeding to Phase 3.

---

## Phase 3: Implement EnvelopeStore for Batch Writes

### Overview
Implement the batch write logic for ProcessedEnvelope messages using the clickhouse-rs 0.14 inserter API with proper DateTime and JSON handling.

### Changes Required

#### 1. Implement EnvelopeStore with batch insert
**File**: `crates/ponix-clickhouse/src/envelope_store.rs`
**Changes**: Create batch write implementation

```rust
use crate::{ClickHouseClient, ProcessedEnvelopeRow};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ponix_proto::envelope::v1::ProcessedEnvelope;
use prost_types::{value::Kind, Timestamp};
use tracing::debug;

pub struct EnvelopeStore {
    client: ClickHouseClient,
    table: String,
}

impl EnvelopeStore {
    pub fn new(client: ClickHouseClient, table: String) -> Self {
        Self { client, table }
    }

    pub async fn store_processed_envelopes(
        &self,
        envelopes: Vec<ProcessedEnvelope>,
    ) -> Result<()> {
        if envelopes.is_empty() {
            return Ok(());
        }

        debug!("Preparing to insert {} envelopes", envelopes.len());

        let mut rows = Vec::with_capacity(envelopes.len());

        for envelope in envelopes {
            // Convert protobuf Struct to JSON string
            let data_json = match envelope.data {
                Some(ref struct_val) => {
                    let fields_map: serde_json::Map<String, serde_json::Value> = struct_val
                        .fields
                        .iter()
                        .filter_map(|(k, v)| {
                            prost_value_to_json(v).map(|json_val| (k.clone(), json_val))
                        })
                        .collect();
                    serde_json::to_string(&fields_map)?
                }
                None => "{}".to_string(),
            };

            let occurred_at = timestamp_to_datetime(
                envelope.occurred_at.as_ref()
                    .context("Missing occurred_at timestamp")?
            )?;

            let processed_at = timestamp_to_datetime(
                envelope.processed_at.as_ref()
                    .context("Missing processed_at timestamp")?
            )?;

            rows.push(ProcessedEnvelopeRow {
                organization_id: envelope.organization_id,
                end_device_id: envelope.end_device_id,
                occurred_at,
                processed_at,
                data: data_json,
            });
        }

        // Perform batch insert using clickhouse-rs 0.14 API
        let mut insert = self
            .client
            .get_client()
            .insert::<ProcessedEnvelopeRow>(&self.table)
            .await?;

        for row in &rows {
            insert.write(row).await?;
        }

        insert.end().await?;

        debug!("Successfully inserted {} envelopes", rows.len());
        Ok(())
    }
}

// Helper function to convert protobuf Timestamp to chrono DateTime
fn timestamp_to_datetime(ts: &Timestamp) -> Result<DateTime<Utc>> {
    let seconds = ts.seconds;
    let nanos = ts.nanos as u32;

    DateTime::from_timestamp(seconds, nanos)
        .context("Invalid timestamp")
}

// Helper function to convert protobuf Value to serde_json::Value
fn prost_value_to_json(value: &prost_types::Value) -> Option<serde_json::Value> {
    match &value.kind {
        Some(Kind::NullValue(_)) => Some(serde_json::Value::Null),
        Some(Kind::NumberValue(n)) => Some(serde_json::json!(n)),
        Some(Kind::StringValue(s)) => Some(serde_json::Value::String(s.clone())),
        Some(Kind::BoolValue(b)) => Some(serde_json::Value::Bool(*b)),
        Some(Kind::StructValue(s)) => {
            let map: serde_json::Map<String, serde_json::Value> = s
                .fields
                .iter()
                .filter_map(|(k, v)| prost_value_to_json(v).map(|json_val| (k.clone(), json_val)))
                .collect();
            Some(serde_json::Value::Object(map))
        }
        Some(Kind::ListValue(l)) => {
            let vec: Vec<serde_json::Value> = l
                .values
                .iter()
                .filter_map(prost_value_to_json)
                .collect();
            Some(serde_json::Value::Array(vec))
        }
        None => None,
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] EnvelopeStore compiles: `cargo build -p ponix-clickhouse`
- [x] No clippy warnings: `cargo clippy -p ponix-clickhouse`
- [x] Helper functions have correct type signatures

#### Manual Verification:
- [x] Code review confirms proper error handling with `Context` trait
- [x] Batch insert logic matches Go implementation pattern

---

## Phase 4: Add Integration Tests

### Overview
Create comprehensive integration tests using testcontainers to verify the complete flow with actual ClickHouse.

### Changes Required

#### 1. Create integration test file
**File**: `crates/ponix-clickhouse/tests/integration.rs`
**Changes**: Add comprehensive integration tests

```rust
use chrono::Utc;
use ponix_clickhouse::{ClickHouseClient, ClickHouseConfig, EnvelopeStore, MigrationRunner};
use ponix_proto::envelope::v1::ProcessedEnvelope;
use prost_types::Timestamp;
use std::time::SystemTime;
use testcontainers::clients::Cli;
use testcontainers_modules::clickhouse::ClickHouse;

#[tokio::test]
async fn test_clickhouse_connection() {
    let docker = Cli::default();
    let clickhouse = docker.run(ClickHouse::default());
    let port = clickhouse.get_host_port_ipv4(8123);

    let client = ClickHouseClient::new(
        &format!("http://localhost:{}", port),
        "default",
        "default",
        "",
    );

    assert!(client.ping().await.is_ok());
}

#[tokio::test]
async fn test_migrations_and_batch_write() {
    let docker = Cli::default();
    let clickhouse = docker.run(ClickHouse::default());
    let port = clickhouse.get_host_port_ipv4(8123);

    // Get goose binary path
    let goose_path = which::which("goose")
        .expect("goose binary not found in PATH");

    // Run migrations
    let migration_runner = MigrationRunner::new(
        goose_path.to_string_lossy().to_string(),
        "crates/ponix-clickhouse/migrations".to_string(),
        format!("localhost:{}", port),
        "default".to_string(),
        "default".to_string(),
        "".to_string(),
    );

    migration_runner.run_migrations().await
        .expect("Migrations should succeed");

    // Create client and store
    let client = ClickHouseClient::new(
        &format!("http://localhost:{}", port),
        "default",
        "default",
        "",
    );

    let store = EnvelopeStore::new(client, "processed_envelopes".to_string());

    // Create test envelope
    let now = SystemTime::now();
    let timestamp = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| Timestamp {
            seconds: d.as_secs() as i64,
            nanos: d.subsec_nanos() as i32,
        })
        .unwrap();

    let envelope = ProcessedEnvelope {
        organization_id: "test-org".to_string(),
        end_device_id: "device-123".to_string(),
        occurred_at: Some(timestamp),
        processed_at: Some(timestamp),
        data: None,
    };

    // Test batch write
    store.store_processed_envelopes(vec![envelope])
        .await
        .expect("Batch write should succeed");

    // Verify data was written (query back)
    // This confirms both write and that serde attributes work correctly
    let result = store.client.get_client()
        .query("SELECT count() FROM processed_envelopes")
        .fetch_one::<u64>()
        .await
        .expect("Query should succeed");

    assert_eq!(result, 1);
}

#[tokio::test]
async fn test_large_batch_write() {
    let docker = Cli::default();
    let clickhouse = docker.run(ClickHouse::default());
    let port = clickhouse.get_host_port_ipv4(8123);

    let goose_path = which::which("goose")
        .expect("goose binary not found in PATH");

    let migration_runner = MigrationRunner::new(
        goose_path.to_string_lossy().to_string(),
        "crates/ponix-clickhouse/migrations".to_string(),
        format!("localhost:{}", port),
        "default".to_string(),
        "default".to_string(),
        "".to_string(),
    );

    migration_runner.run_migrations().await.unwrap();

    let client = ClickHouseClient::new(
        &format!("http://localhost:{}", port),
        "default",
        "default",
        "",
    );

    let store = EnvelopeStore::new(client, "processed_envelopes".to_string());

    // Create 100 test envelopes
    let mut envelopes = Vec::new();
    for i in 0..100 {
        let now = SystemTime::now();
        let timestamp = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| Timestamp {
                seconds: d.as_secs() as i64,
                nanos: d.subsec_nanos() as i32,
            })
            .unwrap();

        envelopes.push(ProcessedEnvelope {
            organization_id: "test-org".to_string(),
            end_device_id: format!("device-{}", i),
            occurred_at: Some(timestamp),
            processed_at: Some(timestamp),
            data: None,
        });
    }

    store.store_processed_envelopes(envelopes)
        .await
        .expect("Large batch write should succeed");

    let count = store.client.get_client()
        .query("SELECT count() FROM processed_envelopes")
        .fetch_one::<u64>()
        .await
        .unwrap();

    assert_eq!(count, 100);
}
```

#### 2. Add which dependency for goose path detection
**File**: `crates/ponix-clickhouse/Cargo.toml`
**Changes**: Add which to dev-dependencies

```toml
[dev-dependencies]
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["clickhouse"] }
tokio-test = "0.4"
which = "7.0"  # Add this
```

### Success Criteria

#### Automated Verification:
- [ ] Integration tests compile: `cargo test -p ponix-clickhouse --no-run`
- [ ] Connection test passes: `cargo test -p ponix-clickhouse test_clickhouse_connection`
- [ ] Migration and batch write test passes: `cargo test -p ponix-clickhouse test_migrations_and_batch_write`
- [ ] Large batch test passes: `cargo test -p ponix-clickhouse test_large_batch_write`
- [ ] All tests pass: `cargo test -p ponix-clickhouse`

#### Manual Verification:
- [ ] Tests demonstrate that DateTime and JSON types work together correctly
- [ ] Verify no "Cannot read all data" or "DateTime as &str" errors appear

**Implementation Note**: These integration tests are critical - they verify the fix for the previous DateTime/JSON compatibility issue. All tests must pass before proceeding.

---

## Phase 5: Add ClickHouse to Docker Compose

### Overview
Add ClickHouse service to docker-compose.deps.yaml for local development.

### Changes Required

#### 1. Update docker-compose.deps.yaml
**File**: `docker/docker-compose.deps.yaml`
**Changes**: Add ClickHouse service

```yaml
name: ponix-deps

services:
  nats:
    image: nats:latest
    container_name: ponix-nats
    ports:
      - "4222:4222"  # Client connections
      - "8222:8222"  # HTTP management
      - "6222:6222"  # Cluster routing
    command:
      - "-js"        # Enable JetStream
      - "-m"         # Enable monitoring
      - "8222"       # Monitoring port
    volumes:
      - nats-data:/data
    restart: unless-stopped
    networks:
      - ponix

  clickhouse:
    image: clickhouse/clickhouse-server:24.10
    container_name: ponix-clickhouse
    ports:
      - "8123:8123"  # HTTP interface
      - "9000:9000"  # Native protocol
    environment:
      CLICKHOUSE_DB: ponix
      CLICKHOUSE_USER: default
      CLICKHOUSE_PASSWORD: ""
      CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT: 1
    volumes:
      - clickhouse-data:/var/lib/clickhouse
    restart: unless-stopped
    networks:
      - ponix
    ulimits:
      nofile:
        soft: 262144
        hard: 262144

networks:
  ponix:
    name: ponix

volumes:
  nats-data:
    driver: local
  clickhouse-data:
    driver: local
```

### Success Criteria

#### Automated Verification:
- [ ] Docker compose file is valid YAML: `docker compose -f docker/docker-compose.deps.yaml config`
- [ ] Services start successfully: `docker compose -f docker/docker-compose.deps.yaml up -d`
- [ ] ClickHouse is healthy: `curl http://localhost:8123/ping`

#### Manual Verification:
- [ ] Can connect to ClickHouse from host machine
- [ ] Both NATS and ClickHouse are on same network
- [ ] Data persists between container restarts

---

## Phase 6: Update Dockerfile to Include Goose Binary

### Overview
Modify the Dockerfile to install Goose in the builder stage and copy it to the runtime image.

### Changes Required

#### 1. Update Dockerfile builder stage
**File**: `crates/ponix-all-in-one/Dockerfile`
**Changes**: Install Goose in builder and copy to runtime

```dockerfile
# syntax=docker/dockerfile:1

# Build stage
FROM rust:1.91-slim-trixie AS builder

# Update ca-certificates and install build dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Install Goose binary
# Using specific version for reproducibility
RUN curl -fsSL https://github.com/pressly/goose/releases/download/v3.22.1/goose_linux_x86_64 \
    -o /usr/local/bin/goose && \
    chmod +x /usr/local/bin/goose

WORKDIR /build

# Configure BSR registry
COPY .cargo/config.toml ./.cargo/config.toml

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build the service with BSR authentication via secret
RUN --mount=type=secret,id=bsr_token \
    if [ -f /run/secrets/bsr_token ]; then \
        export CARGO_REGISTRIES_BUF_TOKEN="Bearer $(cat /run/secrets/bsr_token)" && \
        cargo build --release --bin ponix-all-in-one; \
    else \
        echo "Error: BSR token secret not found" && exit 1; \
    fi

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -ms /bin/bash ponix

# Copy binary from builder
COPY --from=builder /build/target/release/ponix-all-in-one /home/ponix/ponix-all-in-one

# CRITICAL: Copy goose binary from builder for migrations
COPY --from=builder /usr/local/bin/goose /usr/local/bin/goose

# Copy migration files
COPY crates/ponix-clickhouse/migrations /home/ponix/migrations

# Set ownership
RUN chown -R ponix:ponix /home/ponix

USER ponix
WORKDIR /home/ponix

# Default environment variables
ENV PONIX_LOG_LEVEL=info
ENV PONIX_MESSAGE="ponix-all-in-one in Docker"
ENV PONIX_INTERVAL_SECS=5

# ClickHouse defaults
ENV PONIX_CLICKHOUSE_URL=http://clickhouse:8123
ENV PONIX_CLICKHOUSE_DATABASE=ponix
ENV PONIX_CLICKHOUSE_USERNAME=default
ENV PONIX_CLICKHOUSE_PASSWORD=""
ENV PONIX_CLICKHOUSE_MIGRATIONS_DIR=/home/ponix/migrations
ENV PONIX_CLICKHOUSE_GOOSE_BINARY_PATH=/usr/local/bin/goose

ENTRYPOINT ["./ponix-all-in-one"]
```

### Success Criteria

#### Automated Verification:
- [ ] Dockerfile builds successfully with build secrets
- [ ] Goose binary is present in final image: `docker run --rm <image> ls -la /usr/local/bin/goose`
- [ ] Migration files are present: `docker run --rm <image> ls -la /home/ponix/migrations`

#### Manual Verification:
- [ ] Image size is reasonable (goose adds ~10MB)
- [ ] Goose binary has execute permissions
- [ ] Migrations directory is readable by ponix user

---

## Phase 7: Integrate ClickHouse with NATS Consumer

### Overview
Replace the logging-only processor with actual ClickHouse batch writes, integrating the EnvelopeStore into the NATS consumer flow.

### Changes Required

#### 1. Update ponix-all-in-one dependencies
**File**: `crates/ponix-all-in-one/Cargo.toml`
**Changes**: Add ponix-clickhouse dependency

```toml
[dependencies]
ponix-runner = { path = "../runner" }
ponix-nats = { path = "../ponix-nats", features = ["processed-envelope"] }
ponix-clickhouse = { path = "../ponix-clickhouse" }  # Add this
tokio = { workspace = true }
tokio-util = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
serde = { version = "1.0", features = ["derive"] }
config = { version = "0.14", features = ["toml"] }

# Protobuf support
ponix-proto = { package = "ponix_ponix_community_neoeinstein-prost", version = "0.4.0-20251105022230-76aad410c133.1", registry = "buf" }
prost-types = { workspace = true }
xid = { workspace = true }
```

#### 2. Update service configuration
**File**: `crates/ponix-all-in-one/src/config.rs`
**Changes**: Add ClickHouse configuration fields

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceConfig {
    /// Message to print (placeholder for future config)
    #[serde(default = "default_message")]
    pub message: String,

    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Sleep interval in seconds
    #[serde(default = "default_interval")]
    pub interval_secs: u64,

    // NATS configuration
    /// NATS server URL
    #[serde(default = "default_nats_url")]
    pub nats_url: String,

    /// NATS JetStream stream name
    #[serde(default = "default_nats_stream")]
    pub nats_stream: String,

    /// NATS subject pattern for consumer filter
    #[serde(default = "default_nats_subject")]
    pub nats_subject: String,

    /// Batch size for consumer
    #[serde(default = "default_nats_batch_size")]
    pub nats_batch_size: usize,

    /// Max wait time for batches in seconds
    #[serde(default = "default_nats_batch_wait_secs")]
    pub nats_batch_wait_secs: u64,

    /// Startup timeout for initialization operations in seconds
    #[serde(default = "default_startup_timeout_secs")]
    pub startup_timeout_secs: u64,

    // ClickHouse configuration - ADD THESE
    /// ClickHouse HTTP URL
    #[serde(default = "default_clickhouse_url")]
    pub clickhouse_url: String,

    /// ClickHouse database name
    #[serde(default = "default_clickhouse_database")]
    pub clickhouse_database: String,

    /// ClickHouse username
    #[serde(default = "default_clickhouse_username")]
    pub clickhouse_username: String,

    /// ClickHouse password
    #[serde(default = "default_clickhouse_password")]
    pub clickhouse_password: String,

    /// Path to migrations directory
    #[serde(default = "default_clickhouse_migrations_dir")]
    pub clickhouse_migrations_dir: String,

    /// Path to goose binary
    #[serde(default = "default_clickhouse_goose_binary_path")]
    pub clickhouse_goose_binary_path: String,
}

// ... existing default functions ...

// ADD THESE DEFAULT FUNCTIONS
fn default_clickhouse_url() -> String {
    "http://localhost:8123".to_string()
}

fn default_clickhouse_database() -> String {
    "ponix".to_string()
}

fn default_clickhouse_username() -> String {
    "default".to_string()
}

fn default_clickhouse_password() -> String {
    "".to_string()
}

fn default_clickhouse_migrations_dir() -> String {
    "crates/ponix-clickhouse/migrations".to_string()
}

fn default_clickhouse_goose_binary_path() -> String {
    "goose".to_string()
}
```

#### 3. Create ClickHouse-enabled processor
**File**: `crates/ponix-nats/src/processed_envelope_processor.rs`
**Changes**: Add new function that accepts EnvelopeStore

```rust
#[cfg(feature = "processed-envelope")]
use crate::{create_protobuf_processor, BatchProcessor, ProcessingResult, ProtobufHandler};
#[cfg(feature = "processed-envelope")]
use ponix_proto::envelope::v1::ProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use std::sync::Arc;
#[cfg(feature = "processed-envelope")]
use tracing::{error, info};

/// Creates the default batch processor for ProcessedEnvelope messages
/// This processor uses the generic protobuf processor with business logic
/// that logs envelope details
#[cfg(feature = "processed-envelope")]
pub fn create_processed_envelope_processor() -> BatchProcessor {
    // Define business logic handler that works with decoded messages
    let handler: ProtobufHandler<ProcessedEnvelope> = Arc::new(|decoded_messages| {
        Box::pin(async move {
            // Collect indices of all messages to ack
            let mut ack_indices = Vec::new();

            // Process each successfully decoded envelope
            for decoded_msg in decoded_messages {
                let envelope = &decoded_msg.decoded;

                info!(
                    end_device_id = %envelope.end_device_id,
                    organization_id = %envelope.organization_id,
                    occurred_at = ?envelope.occurred_at,
                    processed_at = ?envelope.processed_at,
                    "Processed envelope from NATS"
                );

                // All envelopes are successfully processed, ack them
                ack_indices.push(decoded_msg.index);
            }

            // Return processing result with all acks, no naks
            Ok(ProcessingResult::new(ack_indices, vec![]))
        })
    });

    // Create processor using generic protobuf processor
    create_protobuf_processor(handler)
}

/// Creates a batch processor that writes ProcessedEnvelope messages to ClickHouse
/// This is a generic handler that can be used with any storage implementation
#[cfg(feature = "processed-envelope")]
pub fn create_clickhouse_processor<F, Fut>(store_fn: F) -> BatchProcessor
where
    F: Fn(Vec<ProcessedEnvelope>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let store_fn = Arc::new(store_fn);

    let handler: ProtobufHandler<ProcessedEnvelope> = Arc::new(move |decoded_messages| {
        let store_fn = Arc::clone(&store_fn);
        Box::pin(async move {
            // Extract envelopes from decoded messages
            let envelopes: Vec<ProcessedEnvelope> = decoded_messages
                .iter()
                .map(|msg| msg.decoded.clone())
                .collect();

            let message_count = envelopes.len();

            // Store to ClickHouse
            match store_fn(envelopes).await {
                Ok(_) => {
                    info!("Successfully stored {} envelopes to ClickHouse", message_count);
                    // Ack all messages on success
                    let ack_indices: Vec<usize> = decoded_messages
                        .iter()
                        .map(|msg| msg.index)
                        .collect();
                    Ok(ProcessingResult::new(ack_indices, vec![]))
                }
                Err(e) => {
                    error!("Failed to store envelopes to ClickHouse: {}", e);
                    // Nack all messages on failure so they can be retried
                    let nak_indices: Vec<usize> = decoded_messages
                        .iter()
                        .map(|msg| msg.index)
                        .collect();
                    Ok(ProcessingResult::new(vec![], nak_indices))
                }
            }
        })
    });

    create_protobuf_processor(handler)
}
```

#### 4. Export new processor function
**File**: `crates/ponix-nats/src/lib.rs`
**Changes**: Export the new ClickHouse processor

```rust
#[cfg(feature = "processed-envelope")]
pub use processed_envelope_processor::{
    create_processed_envelope_processor,
    create_clickhouse_processor,  // Add this
};
```

#### 5. Update main.rs to use ClickHouse
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Initialize ClickHouse, run migrations, and use ClickHouse processor

```rust
mod config;

use anyhow::Result;
use ponix_clickhouse::{ClickHouseClient, EnvelopeStore, MigrationRunner};
use ponix_nats::{
    create_clickhouse_processor, NatsClient, NatsConsumer, ProcessedEnvelopeProducer,
};
use ponix_proto::envelope::v1::ProcessedEnvelope;
use ponix_runner::Runner;
use prost_types::Timestamp;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    // Initialize tracing
    let config = match config::ServiceConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    info!("Starting ponix-all-in-one service");
    debug!("Configuration: {:?}", config);

    let startup_timeout = Duration::from_secs(config.startup_timeout_secs);
    debug!("Using startup timeout of {:?} for all initialization", startup_timeout);

    // PHASE 1: Run ClickHouse migrations
    info!("Running ClickHouse migrations...");
    let migration_runner = MigrationRunner::new(
        config.clickhouse_goose_binary_path.clone(),
        config.clickhouse_migrations_dir.clone(),
        config.clickhouse_url.replace("http://", ""),  // Remove http:// for DSN
        config.clickhouse_database.clone(),
        config.clickhouse_username.clone(),
        config.clickhouse_password.clone(),
    );

    if let Err(e) = migration_runner.run_migrations().await {
        error!("Failed to run migrations: {}", e);
        std::process::exit(1);
    }

    // PHASE 2: Initialize ClickHouse client
    info!("Connecting to ClickHouse...");
    let clickhouse_client = ClickHouseClient::new(
        &config.clickhouse_url,
        &config.clickhouse_database,
        &config.clickhouse_username,
        &config.clickhouse_password,
    );

    if let Err(e) = clickhouse_client.ping().await {
        error!("Failed to ping ClickHouse: {}", e);
        std::process::exit(1);
    }
    info!("ClickHouse connection established");

    let envelope_store = EnvelopeStore::new(
        clickhouse_client.clone(),
        "processed_envelopes".to_string(),
    );

    // PHASE 3: Connect to NATS
    let nats_client = match NatsClient::connect(&config.nats_url, startup_timeout).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to NATS: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = nats_client.ensure_stream(&config.nats_stream).await {
        error!("Failed to ensure stream exists: {}", e);
        std::process::exit(1);
    }

    // PHASE 4: Create ClickHouse-enabled processor
    let processor = create_clickhouse_processor(move |envelopes| {
        let store = envelope_store.clone();
        async move {
            store.store_processed_envelopes(envelopes).await
        }
    });

    // Create consumer using trait-based API
    let consumer_client = nats_client.create_consumer_client();
    let consumer = match NatsConsumer::new(
        consumer_client,
        &config.nats_stream,
        "ponix-all-in-one",
        &config.nats_subject,
        config.nats_batch_size,
        config.nats_batch_wait_secs,
        processor,
    )
    .await
    {
        Ok(consumer) => consumer,
        Err(e) => {
            error!("Failed to create consumer: {}", e);
            std::process::exit(1);
        }
    };

    // Create producer using trait-based API
    let publisher_client = nats_client.create_publisher_client();
    let producer = ProcessedEnvelopeProducer::new(
        publisher_client,
        config.nats_stream.clone(),
    );

    // Create runner with producer and consumer processes
    let runner = Runner::new()
        .with_app_process({
            let config = config.clone();
            move |ctx| Box::pin(async move { run_producer_service(ctx, config, producer).await })
        })
        .with_app_process({
            move |ctx| Box::pin(async move { consumer.run(ctx).await })
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

    // Run the service (this will handle the exit)
    runner.run().await;
}

async fn run_producer_service(
    ctx: tokio_util::sync::CancellationToken,
    config: config::ServiceConfig,
    producer: ProcessedEnvelopeProducer,
) -> Result<()> {
    info!("Producer service started successfully");

    let interval = Duration::from_secs(config.interval_secs);

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping producer service");
                break;
            }
            _ = tokio::time::sleep(interval) => {
                let id = xid::new();
                let now = SystemTime::now();
                let timestamp = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| Timestamp {
                        seconds: d.as_secs() as i64,
                        nanos: d.subsec_nanos() as i32,
                    })
                    .ok();

                let envelope = ProcessedEnvelope {
                    end_device_id: id.to_string(),
                    occurred_at: timestamp,
                    data: None,
                    processed_at: timestamp,
                    organization_id: "example-org".to_string(),
                };

                match producer.publish(&envelope).await {
                    Ok(_) => {
                        debug!(
                            end_device_id = %envelope.end_device_id,
                            organization_id = %envelope.organization_id,
                            "{}",
                            config.message
                        );
                    }
                    Err(e) => {
                        error!(
                            end_device_id = %envelope.end_device_id,
                            error = %e,
                            "Failed to publish envelope"
                        );
                    }
                }
            }
        }
    }

    info!("Producer service stopped gracefully");
    Ok(())
}
```

### Success Criteria

#### Automated Verification:
- [ ] ponix-all-in-one compiles: `cargo build -p ponix-all-in-one`
- [ ] No clippy warnings: `cargo clippy -p ponix-all-in-one`
- [ ] Format check passes: `cargo fmt --check`

#### Manual Verification:
- [ ] Start docker-compose deps: `docker compose -f docker/docker-compose.deps.yaml up -d`
- [ ] Run ponix-all-in-one locally
- [ ] Verify migrations run on startup in logs
- [ ] Verify NATS messages are being written to ClickHouse
- [ ] Query ClickHouse to confirm data: `docker exec -it ponix-clickhouse clickhouse-client -q "SELECT count() FROM ponix.processed_envelopes"`
- [ ] Check for proper error handling if ClickHouse is unavailable

**Implementation Note**: This is the critical integration phase. After automated tests pass, manual end-to-end testing is essential to verify the complete flow works as expected.

---

## Phase 8: End-to-End Verification

### Overview
Final comprehensive testing of the entire system to ensure all components work together correctly.

### Changes Required

No code changes - this is a testing and verification phase.

### Success Criteria

#### Automated Verification:
- [ ] All workspace tests pass: `cargo test --workspace`
- [ ] All integration tests pass: `cargo test --workspace -- --ignored` (if any marked as ignored)
- [ ] Clippy passes on workspace: `cargo clippy --workspace`
- [ ] Format check passes: `cargo fmt --check`

#### Manual Verification:
- [ ] **Clean Environment Test**: Stop all containers, remove volumes, restart from scratch
- [ ] **Migration Test**: Verify migrations run successfully on first startup with empty ClickHouse
- [ ] **Data Flow Test**: Producer → NATS → Consumer → ClickHouse full flow works
- [ ] **Data Verification**: Query ClickHouse and verify:
  - All fields are correctly populated
  - DateTime fields have proper values (not null, not corrupted)
  - JSON data field is properly stored
  - organization_id is present in all rows
- [ ] **Error Handling Test**:
  - Stop ClickHouse, verify service handles gracefully (NAKs messages)
  - Restart ClickHouse, verify service recovers
  - Verify NAKed messages are reprocessed
- [ ] **Performance Test**: Verify batch writes are performant (100+ messages/sec)
- [ ] **Docker Build Test**: Build Docker image and verify:
  - Image builds successfully
  - Goose binary is present and functional
  - Migration files are included
  - Service runs correctly in container
- [ ] **Docker Compose Test**: Full stack runs via docker-compose
  - Both services start and stay healthy
  - Migrations run automatically
  - Data flows correctly

**Implementation Note**: This phase validates that the DateTime serde attribute fix resolved the integration issues. The "Data Verification" step is critical to confirm JSON and DateTime types work correctly together.

---

## Testing Strategy

### Unit Tests
- EnvelopeStore helper functions (timestamp conversion, JSON conversion)
- Configuration loading and defaults
- Migration runner command building

### Integration Tests
- ClickHouse connection and ping (uses testcontainers)
- Migration execution (uses testcontainers + goose CLI)
- Batch write with small dataset (1 envelope)
- Batch write with large dataset (100 envelopes)
- Verify DateTime and JSON types work together correctly

### Manual Testing Steps
1. Start local dependencies: `docker compose -f docker/docker-compose.deps.yaml up -d`
2. Run service: `cargo run -p ponix-all-in-one`
3. Monitor logs for successful migration and data writes
4. Query ClickHouse: `docker exec -it ponix-clickhouse clickhouse-client`
   ```sql
   USE ponix;
   SELECT count() FROM processed_envelopes;
   SELECT * FROM processed_envelopes LIMIT 5;
   ```
5. Verify data structure and content correctness
6. Stop ClickHouse, observe NAKs in logs
7. Restart ClickHouse, verify recovery

## Performance Considerations

- Batch size default is 30 (matching Go implementation)
- LZ4 compression enabled for ClickHouse client
- MergeTree engine with monthly partitioning by occurred_at
- Proper indexing on (organization_id, occurred_at, end_device_id)

## Migration Notes

### From Previous Failed Implementation
- The key fix is adding `#[serde(with = "clickhouse::serde::chrono::datetime")]` to DateTime fields
- Re-enable JSON format settings that were removed: `input_format_binary_read_json_as_string` and `output_format_binary_write_json_as_string`
- Use clickhouse-rs 0.14 (not 0.13)

### For Production Deployment
- Goose migrations run automatically on startup
- Migration state is tracked in ClickHouse (goose_db_version table)
- Migrations are idempotent and safe to run multiple times
- No manual migration steps required

## References

- Original issue: https://github.com/ponix-dev/ponix-rs/issues/4
- Integration test findings: `/Users/srall/development/personal/ponix-rs/INTEGRATION_TEST_FINDINGS.md`
- ClickHouse DateTime example: https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_derive_simple.rs
- ClickHouse JSON example: https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_new_json.rs
- Dependency: Issue #3 (NATS consumer - completed)
- Go implementation reference: ponix repository (internal/clickhouse/)
