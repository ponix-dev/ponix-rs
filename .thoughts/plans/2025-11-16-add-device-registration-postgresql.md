# Device Registration with PostgreSQL Storage Implementation Plan

## Overview

Add device registration and storage functionality using PostgreSQL as the database backend. This involves creating two new crates (`goose` and `ponix-postgres`), refactoring the existing `ponix-clickhouse` crate to use shared migration logic, and adding PostgreSQL to the infrastructure.

## Current State Analysis

### Existing Infrastructure:
- **ponix-clickhouse** ([crates/ponix-clickhouse/src/migration.rs:5-58](crates/ponix-clickhouse/src/migration.rs#L5-L58)): Has a ClickHouse-specific `MigrationRunner` that needs to be extracted
- **Workspace pattern**: All crates follow consistent patterns for configuration, error handling, and async operations
- **Integration testing**: Uses testcontainers with feature flag `integration-tests`
- **Protobuf dependencies**: Uses `ponix-proto` from Buf Schema Registry (BSR)

### Key Discoveries:
- Current `MigrationRunner` uses hardcoded DSN format for ClickHouse with `allow_experimental_json_type=1` parameter ([migration.rs:36-39](crates/ponix-clickhouse/src/migration.rs#L36-L39))
- ClickHouse client requires three JSON-related options ([client.rs:17-20](crates/ponix-clickhouse/src/client.rs#L17-L20))
- Integration tests use custom image wrappers for testcontainers ([integration.rs:11-55](crates/ponix-clickhouse/tests/integration.rs#L11-L55))
- BSR authentication uses `CARGO_REGISTRIES_BUF_TOKEN` environment variable
- Current ponix-proto version: `0.4.0-20251105022230-76aad410c133.1`

## Desired End State

After implementation, the system will:
1. Have a generic `goose` crate that supports migrations for any database compatible with goose
2. Have a `ponix-postgres` crate with device storage operations (register, query by ID, list by organization)
3. Have PostgreSQL running in the infrastructure alongside NATS and ClickHouse
4. Have `ponix-clickhouse` refactored to use the shared `goose` crate
5. Have comprehensive integration tests for PostgreSQL operations
6. Have the latest version of `ponix-proto` with the Device message

### Verification:
- Run `cargo build --workspace` successfully
- Run `cargo test --workspace --lib --bins` (unit tests pass)
- Run `cargo test --workspace --features integration-tests -- --test-threads=1` (integration tests pass)
- Start infrastructure with `docker compose -f docker/docker-compose.deps.yaml up -d`
- PostgreSQL accessible on port 5432
- Device operations work as expected in integration tests

## What We're NOT Doing

- Creating the protobuf Device message definition (assumed to exist in latest ponix-proto)
- HTTP API for device registration (future work)
- Integration into `ponix-all-in-one` service (future work)
- Device authentication or authorization
- Additional device metadata beyond ID, organization ID, and name
- Pagination for list operations (may be added later)
- Example usage code or standalone binaries

## Implementation Approach

We'll implement this in three phases:
1. **Phase 1**: Extract and create the generic `goose` crate
2. **Phase 2**: Refactor `ponix-clickhouse` to use the shared `goose` crate
3. **Phase 3**: Create `ponix-postgres` crate with device operations and infrastructure

This approach ensures we maintain a working system throughout the implementation, with each phase being independently testable.

---

## Phase 1: Create Generic Goose Crate

### Overview
Extract the migration runner logic from `ponix-clickhouse` into a new, database-agnostic `goose` crate that can be reused for both ClickHouse and PostgreSQL migrations.

### Changes Required:

#### 1. Create Crate Structure
**Directory**: `crates/goose/`

Create the following files:
- `Cargo.toml`
- `src/lib.rs`
- `README.md` (optional, but recommended)

#### 2. Workspace Configuration
**File**: `Cargo.toml` (root)
**Changes**: Add `crates/goose` to workspace members

```toml
[workspace]
members = [
    "crates/runner",
    "crates/ponix-nats",
    "crates/ponix-clickhouse",
    "crates/ponix-all-in-one",
    "crates/goose",  # Add this line
]
```

#### 3. Goose Crate Cargo.toml
**File**: `crates/goose/Cargo.toml`

```toml
[package]
name = "goose"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
tracing.workspace = true
tokio.workspace = true

[dev-dependencies]
tokio-test = "0.4"
```

#### 4. Generic MigrationRunner Implementation
**File**: `crates/goose/src/lib.rs`

Extract and generalize the migration runner from [ponix-clickhouse/src/migration.rs:5-58](crates/ponix-clickhouse/src/migration.rs#L5-L58):

```rust
use anyhow::{bail, Result};
use std::process::Command;
use tracing::info;

/// Generic migration runner for goose-compatible databases.
///
/// This runner executes goose migrations by spawning the goose binary as a subprocess.
/// It supports any database that goose supports (PostgreSQL, MySQL, SQLite, ClickHouse, etc.)
/// by accepting a generic DSN string.
pub struct MigrationRunner {
    /// Path to the goose binary (e.g., "goose" if in PATH, or absolute path)
    goose_binary_path: String,

    /// Directory containing SQL migration files
    migrations_dir: String,

    /// Database driver name (e.g., "postgres", "clickhouse", "mysql", "sqlite3")
    driver: String,

    /// Database connection string (DSN) - format depends on the driver
    dsn: String,
}

impl MigrationRunner {
    /// Creates a new MigrationRunner
    ///
    /// # Arguments
    /// * `goose_binary_path` - Path to goose binary (e.g., "goose" or "/usr/local/bin/goose")
    /// * `migrations_dir` - Directory containing migration SQL files
    /// * `driver` - Database driver name (e.g., "postgres", "clickhouse")
    /// * `dsn` - Database connection string in driver-specific format
    ///
    /// # Example
    /// ```
    /// let runner = MigrationRunner::new(
    ///     "goose".to_string(),
    ///     "migrations/".to_string(),
    ///     "postgres".to_string(),
    ///     "postgres://user:pass@localhost:5432/dbname?sslmode=disable".to_string(),
    /// );
    /// ```
    pub fn new(
        goose_binary_path: String,
        migrations_dir: String,
        driver: String,
        dsn: String,
    ) -> Self {
        Self {
            goose_binary_path,
            migrations_dir,
            driver,
            dsn,
        }
    }

    /// Runs all pending migrations
    ///
    /// Executes `goose -dir {migrations_dir} {driver} {dsn} up`
    ///
    /// # Errors
    /// Returns an error if:
    /// - The goose binary is not found
    /// - Migration execution fails
    /// - Database connection fails
    pub async fn run_migrations(&self) -> Result<()> {
        info!("Running migrations from directory: {}", self.migrations_dir);

        let output = Command::new(&self.goose_binary_path)
            .arg("-dir")
            .arg(&self.migrations_dir)
            .arg(&self.driver)
            .arg(&self.dsn)
            .arg("up")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "Migration failed.\nstdout: {}\nstderr: {}",
                stdout,
                stderr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("Migrations completed successfully:\n{}", stdout);

        Ok(())
    }

    /// Rolls back the most recent migration
    ///
    /// Executes `goose -dir {migrations_dir} {driver} {dsn} down`
    pub async fn rollback_migration(&self) -> Result<()> {
        info!("Rolling back most recent migration");

        let output = Command::new(&self.goose_binary_path)
            .arg("-dir")
            .arg(&self.migrations_dir)
            .arg(&self.driver)
            .arg(&self.dsn)
            .arg("down")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "Rollback failed.\nstdout: {}\nstderr: {}",
                stdout,
                stderr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("Rollback completed successfully:\n{}", stdout);

        Ok(())
    }

    /// Gets the current migration status
    ///
    /// Executes `goose -dir {migrations_dir} {driver} {dsn} status`
    pub async fn migration_status(&self) -> Result<String> {
        let output = Command::new(&self.goose_binary_path)
            .arg("-dir")
            .arg(&self.migrations_dir)
            .arg(&self.driver)
            .arg(&self.dsn)
            .arg("status")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to get migration status: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_runner_creation() {
        let runner = MigrationRunner::new(
            "goose".to_string(),
            "migrations/".to_string(),
            "postgres".to_string(),
            "postgres://localhost/test".to_string(),
        );

        assert_eq!(runner.goose_binary_path, "goose");
        assert_eq!(runner.driver, "postgres");
    }
}
```

### Success Criteria:

#### Automated Verification:
- [x] Crate builds successfully: `cargo build -p goose`
- [x] Unit tests pass: `cargo test -p goose`
- [x] Workspace builds: `cargo build --workspace`
- [x] No linting errors: `cargo clippy -p goose`
- [x] Code is formatted: `cargo fmt --check`

#### Manual Verification:
- [ ] API design is clear and well-documented
- [ ] Generic enough to support multiple database types
- [ ] Follows existing codebase patterns for error handling and logging

---

## Phase 2: Refactor ponix-clickhouse to Use Goose Crate

### Overview
Update `ponix-clickhouse` to use the new shared `goose` crate instead of its internal `MigrationRunner` implementation.

### Changes Required:

#### 1. Update Dependencies
**File**: `crates/ponix-clickhouse/Cargo.toml`
**Changes**: Add goose crate dependency

```toml
[dependencies]
# ... existing dependencies ...
goose = { path = "../goose" }
```

#### 2. Remove Old Migration Module
**File**: `crates/ponix-clickhouse/src/migration.rs`
**Action**: Delete this file entirely

#### 3. Update Module Exports
**File**: `crates/ponix-clickhouse/src/lib.rs`
**Changes**: Update exports to use goose crate

```rust
pub use clickhouse_client::ClickHouseClient;
pub use config::ClickHouseConfig;
pub use envelope_store::EnvelopeStore;
pub use models::ProcessedEnvelopeRow;

// Re-export the MigrationRunner from goose for convenience
pub use goose::MigrationRunner;
```

#### 4. Update Integration Tests
**File**: `crates/ponix-clickhouse/tests/integration.rs`
**Changes**: Import MigrationRunner from goose

Update the imports section (around line 4):
```rust
use goose::MigrationRunner;
use ponix_clickhouse::{ClickHouseClient, EnvelopeStore};
```

No other changes needed in the test file - the API remains the same.

#### 5. Update Main Service
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Update import for MigrationRunner

Update imports (around line 5):
```rust
use goose::MigrationRunner;
use ponix_clickhouse::{ClickHouseClient, EnvelopeStore};
```

Update MigrationRunner creation ([main.rs:42-56](crates/ponix-all-in-one/src/main.rs#L42-L56)) to use the new API:
```rust
let migration_runner = MigrationRunner::new(
    config.clickhouse_goose_binary_path.clone(),
    config.clickhouse_migrations_dir.clone(),
    "clickhouse".to_string(),  // driver name
    format!(
        "clickhouse://{}:{}@{}/{}?allow_experimental_json_type=1",
        config.clickhouse_username,
        config.clickhouse_password,
        config.clickhouse_native_url,
        config.clickhouse_database
    ),  // DSN with ClickHouse-specific parameters
);
```

### Success Criteria:

#### Automated Verification:
- [ ] Workspace builds successfully: `cargo build --workspace`
- [ ] Unit tests pass: `cargo test --workspace --lib --bins`
- [ ] ClickHouse integration tests pass: `cargo test -p ponix-clickhouse --features integration-tests -- --test-threads=1`
- [ ] No linting errors: `cargo clippy --workspace`
- [ ] Code is formatted: `cargo fmt --check`

#### Manual Verification:
- [ ] All tests produce the same results as before refactoring
- [ ] No functionality is broken or changed
- [ ] Migration behavior is identical to before

**Implementation Note**: After completing this phase and all automated verification passes, confirm that ClickHouse migrations still work correctly before proceeding to Phase 3.

---

## Phase 3: Create ponix-postgres Crate with Device Operations

### Overview
Create the new `ponix-postgres` crate for device storage operations, add PostgreSQL to infrastructure, and implement comprehensive integration tests.

### Changes Required:

#### 1. Update ponix-proto Dependency
**File**: `crates/ponix-clickhouse/Cargo.toml`, `crates/ponix-nats/Cargo.toml`, `crates/ponix-all-in-one/Cargo.toml`
**Changes**: Update to latest version to get Device message

Update the version to the latest (this will need to be checked at implementation time):
```toml
ponix-proto = { package = "ponix_ponix_community_neoeinstein-prost", version = "0.5.0-XXXXXXX", registry = "buf" }
```

Run `cargo update ponix_ponix_community_neoeinstein-prost` to pull the latest version.

#### 2. Create Crate Structure
**Directory**: `crates/ponix-postgres/`

Create the following files:
- `Cargo.toml`
- `src/lib.rs`
- `src/client.rs` - PostgreSQL connection wrapper
- `src/config.rs` - Configuration struct
- `src/device_store.rs` - Device storage operations
- `src/models.rs` - Rust data models
- `migrations/` - Directory for SQL migrations
- `tests/integration.rs` - Integration tests

#### 3. Workspace Configuration
**File**: `Cargo.toml` (root)
**Changes**: Add `crates/ponix-postgres` to workspace members

```toml
[workspace]
members = [
    "crates/runner",
    "crates/ponix-nats",
    "crates/ponix-clickhouse",
    "crates/ponix-all-in-one",
    "crates/goose",
    "crates/ponix-postgres",  # Add this line
]
```

#### 4. Postgres Crate Cargo.toml
**File**: `crates/ponix-postgres/Cargo.toml`

```toml
[package]
name = "ponix-postgres"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
# PostgreSQL client
tokio-postgres = { version = "0.7", features = ["with-chrono-0_4", "with-serde_json-1"] }
# Connection pooling
deadpool-postgres = "0.14"

# Async runtime
tokio.workspace = true

# Error handling
anyhow.workspace = true

# Logging
tracing.workspace = true

# Serialization
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true

# Time handling
chrono = { workspace = true, features = ["serde"] }

# Protobuf
ponix-proto = { package = "ponix_ponix_community_neoeinstein-prost", version = "0.4.0-20251105022230-76aad410c133.1", registry = "buf" }
prost.workspace = true
prost-types.workspace = true

# Migration runner
goose = { path = "../goose" }

[dev-dependencies]
tokio-test = "0.4"
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres"] }

[features]
default = []
integration-tests = []
```

#### 5. PostgreSQL Client
**File**: `crates/ponix-postgres/src/client.rs`

```rust
use anyhow::Result;
use deadpool_postgres::{Config, Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;
use tracing::info;

/// PostgreSQL client wrapper with connection pooling
#[derive(Clone)]
pub struct PostgresClient {
    pool: Pool,
}

impl PostgresClient {
    /// Creates a new PostgreSQL client with connection pooling
    ///
    /// # Arguments
    /// * `host` - Database host (e.g., "localhost")
    /// * `port` - Database port (e.g., 5432)
    /// * `database` - Database name
    /// * `username` - Database username
    /// * `password` - Database password
    /// * `max_pool_size` - Maximum number of connections in the pool
    pub fn new(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
        max_pool_size: usize,
    ) -> Result<Self> {
        let mut cfg = Config::new();
        cfg.host = Some(host.to_string());
        cfg.port = Some(port);
        cfg.dbname = Some(database.to_string());
        cfg.user = Some(username.to_string());
        cfg.password = Some(password.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;

        // Set pool size
        pool.resize(max_pool_size);

        Ok(Self { pool })
    }

    /// Pings the database to verify connectivity
    pub async fn ping(&self) -> Result<()> {
        let client = self.pool.get().await?;
        client.execute("SELECT 1", &[]).await?;
        info!("PostgreSQL connection successful");
        Ok(())
    }

    /// Gets a connection from the pool
    pub async fn get_connection(&self) -> Result<deadpool_postgres::Client> {
        Ok(self.pool.get().await?)
    }
}
```

#### 6. Configuration
**File**: `crates/ponix-postgres/src/config.rs`

```rust
use serde::{Deserialize, Serialize};

/// PostgreSQL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub max_pool_size: usize,
    pub migrations_dir: String,
    pub goose_binary_path: String,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "ponix".to_string(),
            username: "ponix".to_string(),
            password: "ponix".to_string(),
            max_pool_size: 10,
            migrations_dir: "crates/ponix-postgres/migrations".to_string(),
            goose_binary_path: "goose".to_string(),
        }
    }
}
```

#### 7. Data Models
**File**: `crates/ponix-postgres/src/models.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Device row for PostgreSQL storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRow {
    pub device_id: String,
    pub organization_id: String,
    pub device_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### 8. Device Store
**File**: `crates/ponix-postgres/src/device_store.rs`

```rust
use anyhow::{Context, Result};
use chrono::Utc;
use ponix_proto::device::v1::Device;
use tracing::debug;

use crate::client::PostgresClient;
use crate::models::DeviceRow;

/// Device storage operations for PostgreSQL
#[derive(Clone)]
pub struct DeviceStore {
    client: PostgresClient,
}

impl DeviceStore {
    /// Creates a new DeviceStore
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }

    /// Registers a new device
    ///
    /// # Arguments
    /// * `device` - The Device protobuf message to store
    ///
    /// # Returns
    /// `Ok(())` if successful, error otherwise
    ///
    /// # Errors
    /// - Device ID already exists (unique constraint violation)
    /// - Database connection issues
    pub async fn register_device(&self, device: &Device) -> Result<()> {
        let conn = self.client.get_connection().await?;
        let now = Utc::now();

        conn.execute(
            "INSERT INTO devices (device_id, organization_id, device_name, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &device.device_id,
                &device.organization_id,
                &device.device_name,
                &now,
                &now,
            ],
        )
        .await
        .context("Failed to insert device")?;

        debug!("Registered device: {}", device.device_id);

        Ok(())
    }

    /// Queries a device by device ID
    ///
    /// # Arguments
    /// * `device_id` - The unique device identifier
    ///
    /// # Returns
    /// `Ok(Some(Device))` if found, `Ok(None)` if not found, error on database issues
    pub async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>> {
        let conn = self.client.get_connection().await?;

        let row = conn
            .query_opt(
                "SELECT device_id, organization_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE device_id = $1",
                &[&device_id],
            )
            .await
            .context("Failed to query device")?;

        match row {
            Some(row) => {
                let device = Device {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    device_name: row.get(2),
                };
                Ok(Some(device))
            }
            None => Ok(None),
        }
    }

    /// Lists all devices for an organization
    ///
    /// # Arguments
    /// * `organization_id` - The organization identifier
    ///
    /// # Returns
    /// Vector of Device messages for the organization
    pub async fn list_devices_by_organization(
        &self,
        organization_id: &str,
    ) -> Result<Vec<Device>> {
        let conn = self.client.get_connection().await?;

        let rows = conn
            .query(
                "SELECT device_id, organization_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE organization_id = $1
                 ORDER BY created_at DESC",
                &[&organization_id],
            )
            .await
            .context("Failed to query devices by organization")?;

        let devices = rows
            .iter()
            .map(|row| Device {
                device_id: row.get(0),
                organization_id: row.get(1),
                device_name: row.get(2),
            })
            .collect();

        debug!(
            "Found {} devices for organization: {}",
            rows.len(),
            organization_id
        );

        Ok(devices)
    }

    /// Updates a device name
    ///
    /// # Arguments
    /// * `device_id` - The device to update
    /// * `new_name` - The new device name
    ///
    /// # Returns
    /// `Ok(())` if successful, error if device not found or database issues
    pub async fn update_device_name(&self, device_id: &str, new_name: &str) -> Result<()> {
        let conn = self.client.get_connection().await?;
        let now = Utc::now();

        let rows_affected = conn
            .execute(
                "UPDATE devices
                 SET device_name = $1, updated_at = $2
                 WHERE device_id = $3",
                &[&new_name, &now, &device_id],
            )
            .await
            .context("Failed to update device")?;

        if rows_affected == 0 {
            anyhow::bail!("Device not found: {}", device_id);
        }

        debug!("Updated device name: {}", device_id);

        Ok(())
    }
}
```

#### 9. Library Exports
**File**: `crates/ponix-postgres/src/lib.rs`

```rust
mod client;
mod config;
mod device_store;
mod models;

pub use client::PostgresClient;
pub use config::PostgresConfig;
pub use device_store::DeviceStore;
pub use goose::MigrationRunner;
pub use models::DeviceRow;
```

#### 10. Database Migration
**File**: `crates/ponix-postgres/migrations/20251116000000_create_devices_table.sql`

```sql
-- +goose Up
-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    device_name TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Index for querying devices by organization
CREATE INDEX idx_devices_organization_id ON devices(organization_id);

-- Index for sorting by creation time
CREATE INDEX idx_devices_created_at ON devices(created_at DESC);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP INDEX IF EXISTS idx_devices_created_at;
DROP INDEX IF EXISTS idx_devices_organization_id;
DROP TABLE IF EXISTS devices;
-- +goose StatementEnd
```

#### 11. Integration Tests
**File**: `crates/ponix-postgres/tests/integration.rs`

```rust
use ponix_postgres::{DeviceStore, MigrationRunner, PostgresClient};
use ponix_proto::device::v1::Device;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_postgres_connection() {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .unwrap();

    client.ping().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_device_registration_and_query() {
    // Start PostgreSQL container
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    // Create client
    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .unwrap();

    client.ping().await.unwrap();

    // Run migrations
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        host, port
    );
    let migration_runner = MigrationRunner::new(
        "goose".to_string(),
        migrations_dir,
        "postgres".to_string(),
        dsn,
    );

    migration_runner.run_migrations().await.unwrap();

    // Create device store
    let device_store = DeviceStore::new(client.clone());

    // Test 1: Register a device
    let device = Device {
        device_id: "device-001".to_string(),
        organization_id: "org-001".to_string(),
        device_name: "Test Device 1".to_string(),
    };

    device_store.register_device(&device).await.unwrap();

    // Test 2: Query device by ID
    let retrieved = device_store
        .get_device_by_id("device-001")
        .await
        .unwrap();

    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.device_id, "device-001");
    assert_eq!(retrieved.organization_id, "org-001");
    assert_eq!(retrieved.device_name, "Test Device 1");

    // Test 3: Query non-existent device
    let not_found = device_store.get_device_by_id("device-999").await.unwrap();
    assert!(not_found.is_none());

    // Test 4: Register another device in the same organization
    let device2 = Device {
        device_id: "device-002".to_string(),
        organization_id: "org-001".to_string(),
        device_name: "Test Device 2".to_string(),
    };

    device_store.register_device(&device2).await.unwrap();

    // Test 5: List devices by organization
    let devices = device_store
        .list_devices_by_organization("org-001")
        .await
        .unwrap();

    assert_eq!(devices.len(), 2);
    assert!(devices.iter().any(|d| d.device_id == "device-001"));
    assert!(devices.iter().any(|d| d.device_id == "device-002"));

    // Test 6: Update device name
    device_store
        .update_device_name("device-001", "Updated Device Name")
        .await
        .unwrap();

    let updated = device_store
        .get_device_by_id("device-001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.device_name, "Updated Device Name");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_devices_for_empty_organization() {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .unwrap();

    // Run migrations
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        host, port
    );
    let migration_runner = MigrationRunner::new(
        "goose".to_string(),
        migrations_dir,
        "postgres".to_string(),
        dsn,
    );

    migration_runner.run_migrations().await.unwrap();

    let device_store = DeviceStore::new(client);

    let devices = device_store
        .list_devices_by_organization("org-999")
        .await
        .unwrap();

    assert_eq!(devices.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_duplicate_device_id_fails() {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .unwrap();

    // Run migrations
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        host, port
    );
    let migration_runner = MigrationRunner::new(
        "goose".to_string(),
        migrations_dir,
        "postgres".to_string(),
        dsn,
    );

    migration_runner.run_migrations().await.unwrap();

    let device_store = DeviceStore::new(client);

    let device = Device {
        device_id: "device-duplicate".to_string(),
        organization_id: "org-001".to_string(),
        device_name: "Test Device".to_string(),
    };

    // First registration should succeed
    device_store.register_device(&device).await.unwrap();

    // Second registration with same ID should fail
    let result = device_store.register_device(&device).await;
    assert!(result.is_err());
}
```

#### 12. Add PostgreSQL to Infrastructure
**File**: `docker/docker-compose.deps.yaml`
**Changes**: Add PostgreSQL service

Add this section after the ClickHouse service (around line 40):

```yaml
  postgres:
    image: postgres:16
    container_name: ponix-postgres
    environment:
      POSTGRES_USER: ponix
      POSTGRES_PASSWORD: ponix
      POSTGRES_DB: ponix
    ports:
      - "5432:5432"
    volumes:
      - postgres-data:/var/lib/postgresql/data
    networks:
      - ponix
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ponix"]
      interval: 10s
      timeout: 5s
      retries: 5
```

Add the volume to the volumes section (around line 48):

```yaml
volumes:
  nats-data:
    driver: local
  clickhouse-data:
    driver: local
  postgres-data:  # Add this line
    driver: local
```

### Success Criteria:

#### Automated Verification:
- [ ] Workspace builds successfully: `cargo build --workspace`
- [ ] Unit tests pass: `cargo test --workspace --lib --bins`
- [ ] Postgres integration tests pass: `cargo test -p ponix-postgres --features integration-tests -- --test-threads=1`
- [ ] All integration tests pass: `cargo test --workspace --features integration-tests -- --test-threads=1`
- [ ] No linting errors: `cargo clippy --workspace`
- [ ] Code is formatted: `cargo fmt --check`

#### Manual Verification:
- [ ] PostgreSQL starts successfully via docker compose: `docker compose -f docker/docker-compose.deps.yaml up -d postgres`
- [ ] Can connect to PostgreSQL: `docker exec -it ponix-postgres psql -U ponix -d ponix`
- [ ] Migrations create the devices table with correct schema
- [ ] Device registration works and enforces unique device_id constraint
- [ ] Query operations return correct results
- [ ] List operations are sorted by creation time (newest first)
- [ ] Update operations modify the updated_at timestamp

**Implementation Note**: After completing this phase, verify that all three databases (NATS, ClickHouse, PostgreSQL) can run concurrently without port conflicts or resource issues.

---

## Testing Strategy

### Unit Tests:
- **goose crate**: Test MigrationRunner creation and validation
- **ponix-postgres**: Test model serialization/deserialization
- **Configuration**: Test default values and parsing

### Integration Tests:
- **goose**: Verify migration execution with test database
- **ponix-postgres**:
  - Connection and ping
  - Device registration and retrieval
  - List operations
  - Update operations
  - Duplicate device ID handling
  - Empty organization handling

### Manual Testing Steps:
1. Start infrastructure: `docker compose -f docker/docker-compose.deps.yaml up -d`
2. Verify all services are healthy: `docker compose -f docker/docker-compose.deps.yaml ps`
3. Run integration tests: `cargo test --workspace --features integration-tests -- --test-threads=1`
4. Connect to PostgreSQL and verify schema: `docker exec -it ponix-postgres psql -U ponix -d ponix -c "\d devices"`
5. Verify indexes exist: `docker exec -it ponix-postgres psql -U ponix -d ponix -c "\di"`

## Performance Considerations

- **Connection Pooling**: PostgreSQL client uses deadpool-postgres with configurable pool size (default 10)
- **Indexes**: Created on organization_id and created_at for efficient queries
- **Batch Operations**: Not implemented in initial version (single device operations only)
- **Query Optimization**: List operations use DESC ordering on created_at index for efficient newest-first retrieval

## Migration Notes

### Migration Workflow:
1. goose maintains a version table to track applied migrations
2. Migrations are applied in order by timestamp prefix
3. Each migration includes Up and Down sections for rollback capability
4. Running migrations is idempotent - safe to run multiple times

### Adding New Migrations:
```bash
# Create a new migration
goose -dir crates/ponix-postgres/migrations create add_new_field sql

# Apply migrations
goose -dir crates/ponix-postgres/migrations postgres "postgres://ponix:ponix@localhost:5432/ponix?sslmode=disable" up

# Rollback last migration
goose -dir crates/ponix-postgres/migrations postgres "postgres://ponix:ponix@localhost:5432/ponix?sslmode=disable" down
```

## References

- Original ticket: Issue #16 - Add Device Registration with PostgreSQL Storage
- ClickHouse implementation pattern: [crates/ponix-clickhouse](crates/ponix-clickhouse)
- Migration runner pattern: [crates/ponix-clickhouse/src/migration.rs:5-58](crates/ponix-clickhouse/src/migration.rs#L5-L58)
- Integration test pattern: [crates/ponix-clickhouse/tests/integration.rs](crates/ponix-clickhouse/tests/integration.rs)
- Workspace structure: [Cargo.toml](Cargo.toml)
- Docker infrastructure: [docker/docker-compose.deps.yaml](docker/docker-compose.deps.yaml)
