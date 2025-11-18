# gRPC API for Device Management Implementation Plan

## Overview

Implement a gRPC API layer for device management that connects the protobuf contract (from BSR) with the PostgreSQL storage layer. This implementation introduces domain-driven design principles with proper separation of concerns across handler, domain, and data layers.

## Current State Analysis

### What Exists Now

**PostgreSQL Implementation (ponix-rs#16)**:
- `crates/ponix-postgres/` crate is fully implemented with:
  - `DeviceStore` providing CRUD operations: `register_device`, `get_device_by_id`, `list_devices_by_organization`, `update_device_name`
  - PostgreSQL client with connection pooling (deadpool-postgres)
  - Migration file: `migrations/20251116000000_create_devices_table.sql`
  - Database schema with devices table, organization_id index, created_at index
  - Integration tests using testcontainers pattern
  - File: [device_store.rs](crates/ponix-postgres/src/device_store.rs)

**Local Device Model**:
- Custom `Device` struct in [models.rs](crates/ponix-postgres/src/models.rs:7-11) with fields:
  - `device_id: String`
  - `organization_id: String`
  - `device_name: String`
- Comment indicates this will be replaced with protobuf version

**Protobuf BSR Integration**:
- Package: `ponix_ponix_community_neoeinstein-prost` version `0.4.0-20251105022230-76aad410c133.1`
- Registry: `buf` (requires BSR_TOKEN authentication)
- Pattern: [ponix-nats/Cargo.toml:20](crates/ponix-nats/Cargo.toml#L20)

**Service Infrastructure**:
- Runner pattern for process lifecycle management: [runner/lib.rs](crates/runner/src/lib.rs)
- Configuration from environment variables with `PONIX_` prefix: [config.rs](crates/ponix-all-in-one/src/config.rs)
- Phased startup pattern in [main.rs](crates/ponix-all-in-one/src/main.rs:42-165)
- Docker Compose setup for local development

### What's Missing

1. **BSR Tonic Package**: Need to pull `ponix_ponix_community_neoeinstein-tonic` package from BSR for gRPC service definitions
2. **ponix-domain Crate**: Domain layer with repository traits, domain types, and business logic
3. **ponix-grpc Crate**: Tonic-based gRPC server with DeviceService implementation
4. **Service Orchestration**: Domain service layer that orchestrates business logic on top of repository
5. **Type Mappings**: Conversion between Proto/Domain/Database layers using From/Into traits
6. **Error Handling**: Domain error types and mapping to gRPC status codes
7. **gRPC Integration**: Wire gRPC server into ponix-all-in-one as a runner process

### Key Constraints Discovered

- Field name mismatch: Database uses `device_name` but protobuf uses `name` (will handle with mapping)
- BSR authentication required: Must set `BSR_TOKEN` environment variable
- Mockall testing pattern: Traits must use `#[cfg_attr(test, mockall::automock)]` for unit tests
- Runner pattern: All processes run concurrently; if any fails, all are cancelled
- Configuration pattern: All config loaded from env vars with defaults, using `config` crate

## Desired End State

A fully functional gRPC API layer for device management with:

1. **Three-Layer Architecture**:
   ```
   ponix-grpc (handlers)
       ↓ Proto → Domain mapping
   ponix-domain (business logic + repository trait)
       ↓ Domain types
   ponix-postgres (infrastructure - implements repository trait)
       ↓ Domain → Database mapping
   PostgreSQL
   ```

2. **gRPC API Endpoints**:
   - `CreateDevice(CreateEndDeviceRequest) → CreateEndDeviceResponse`
   - `GetDevice(GetEndDeviceRequest) → GetEndDeviceResponse`
   - `ListDevices(ListEndDevicesRequest) → ListEndDevicesResponse`

3. **Domain-Driven Design**:
   - Repository trait defines contract (dependency inversion)
   - Domain service orchestrates business logic
   - Clear type boundaries with explicit conversions

4. **Testing Coverage**:
   - Unit tests in domain layer using mockall for repository mocks
   - Unit tests in gRPC layer for proto/domain mapping
   - Integration tests in postgres layer with testcontainers
   - All tests passing with proper error handling

### Verification

The implementation is complete when:
- `make test` passes for all workspace members
- gRPC server starts successfully in `ponix-all-in-one`
- Can call all three RPC methods via grpcurl or similar client
- Proper error responses for invalid requests (NOT_FOUND, ALREADY_EXISTS, etc.)
- Docker Compose exposes gRPC port and service is accessible
- Repository pattern allows easy swapping of storage backend

## What We're NOT Doing

- Advanced domain validation or value objects (keeping domain types simple with String fields)
- Authentication/authorization mechanisms
- Additional device operations beyond Create/Get/List (no Update/Delete)
- Pagination for list operations (can add later)
- Deployment configuration beyond local docker-compose
- Performance optimization, caching, or connection pooling beyond what exists
- Metrics/observability/tracing (can leverage existing patterns later)
- Transaction handling across multiple operations
- Bi-directional streaming or client streaming endpoints
- TLS/SSL configuration for gRPC (local development only)

## Implementation Approach

### Layered Architecture Strategy

1. **Bottom-Up Implementation**: Start with domain layer (contracts), then infrastructure (postgres), then handlers (grpc)
2. **Dependency Inversion**: Domain defines repository trait; postgres implements it (domain has no knowledge of postgres)
3. **Type Mapping Strategy**: Use Rust's `From`/`Into` traits at each boundary:
   - Proto ↔ Domain: In grpc handlers
   - Domain ↔ Database: In postgres repository implementation
4. **Error Handling Flow**:
   - Postgres returns `anyhow::Result<T>`
   - Domain wraps in domain-specific errors
   - gRPC handlers map to tonic::Status codes
5. **Testing Strategy**: Unit tests with mocks at each layer, integration tests at postgres layer only

### Key Technical Decisions

- **Domain Types**: Simple structs with `String` fields (can evolve to newtypes later)
- **Service Layer**: Domain service orchestrates repository calls and business logic
- **BSR Packages**: Both prost (types) and tonic (service) packages from BSR
- **gRPC Port**: 50051 (configurable via `PONIX_GRPC_PORT`)
- **Error Mapping**: Best-effort mapping to standard gRPC codes (NOT_FOUND, ALREADY_EXISTS, INTERNAL, INVALID_ARGUMENT)

---

## Phase 1: Domain Layer Foundation

### Overview
Create the `ponix-domain` crate with domain types, repository trait, domain service, and error types. This phase establishes the contracts that other layers depend on.

### Changes Required

#### 1. Create ponix-domain Crate Structure

**File**: `crates/ponix-domain/Cargo.toml`
**Changes**: Create new crate manifest

```toml
[package]
name = "ponix-domain"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
anyhow = { workspace = true }
thiserror = { workspace = true }
async-trait = "0.1"

# Protobuf types for domain models
ponix-proto = { package = "ponix_ponix_community_neoeinstein-prost", version = "0.4.0-20251105022230-76aad410c133.1", registry = "buf" }
prost-types = { workspace = true }

[dev-dependencies]
mockall = "0.13"
tokio = { workspace = true }
```

#### 2. Domain Types Module

**File**: `crates/ponix-domain/src/types.rs`
**Changes**: Create domain types

```rust
use ponix_proto::end_device::v1::EndDevice as ProtoEndDevice;

/// Domain representation of a Device
/// Simple String types for now - can evolve to newtypes later
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub device_id: String,
    pub organization_id: String,
    pub name: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Input for creating a new device
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDeviceInput {
    pub device_id: String,
    pub organization_id: String,
    pub name: String,
}

/// Input for retrieving a device
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetDeviceInput {
    pub device_id: String,
}

/// Input for listing devices by organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDevicesInput {
    pub organization_id: String,
}
```

#### 3. Domain Error Types

**File**: `crates/ponix-domain/src/error.rs`
**Changes**: Define domain-specific errors

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Device already exists: {0}")]
    DeviceAlreadyExists(String),

    #[error("Invalid device ID: {0}")]
    InvalidDeviceId(String),

    #[error("Invalid organization ID: {0}")]
    InvalidOrganizationId(String),

    #[error("Invalid device name: {0}")]
    InvalidDeviceName(String),

    #[error("Repository error: {0}")]
    RepositoryError(#[from] anyhow::Error),
}

pub type DomainResult<T> = Result<T, DomainError>;
```

#### 4. Repository Trait (Dependency Inversion)

**File**: `crates/ponix-domain/src/repository.rs`
**Changes**: Define repository contract

```rust
use async_trait::async_trait;
use crate::error::DomainResult;
use crate::types::{CreateDeviceInput, Device, GetDeviceInput, ListDevicesInput};

/// Repository trait for device storage operations
/// Infrastructure layer (e.g., ponix-postgres) implements this trait
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Create a new device
    async fn create_device(&self, input: CreateDeviceInput) -> DomainResult<Device>;

    /// Get a device by ID
    async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Option<Device>>;

    /// List all devices for an organization
    async fn list_devices(&self, input: ListDevicesInput) -> DomainResult<Vec<Device>>;
}
```

#### 5. Domain Service (Business Logic Orchestrator)

**File**: `crates/ponix-domain/src/service.rs`
**Changes**: Implement domain service

```rust
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info};

use crate::error::{DomainError, DomainResult};
use crate::repository::DeviceRepository;
use crate::types::{CreateDeviceInput, Device, GetDeviceInput, ListDevicesInput};

/// Domain service for device management business logic
/// This is the orchestration layer that handlers call
pub struct DeviceService {
    repository: Arc<dyn DeviceRepository>,
}

impl DeviceService {
    pub fn new(repository: Arc<dyn DeviceRepository>) -> Self {
        Self { repository }
    }

    /// Create a new device with business logic validation
    pub async fn create_device(&self, input: CreateDeviceInput) -> DomainResult<Device> {
        // Business logic: validate inputs
        if input.device_id.is_empty() {
            return Err(DomainError::InvalidDeviceId("Device ID cannot be empty".to_string()));
        }

        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId("Organization ID cannot be empty".to_string()));
        }

        if input.name.is_empty() {
            return Err(DomainError::InvalidDeviceName("Device name cannot be empty".to_string()));
        }

        debug!(device_id = %input.device_id, organization_id = %input.organization_id, "Creating device");

        let device = self.repository.create_device(input).await?;

        info!(device_id = %device.device_id, "Device created successfully");
        Ok(device)
    }

    /// Get a device by ID
    pub async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Device> {
        if input.device_id.is_empty() {
            return Err(DomainError::InvalidDeviceId("Device ID cannot be empty".to_string()));
        }

        debug!(device_id = %input.device_id, "Getting device");

        let device = self.repository.get_device(input).await?
            .ok_or_else(|| DomainError::DeviceNotFound("Device not found".to_string()))?;

        Ok(device)
    }

    /// List devices for an organization
    pub async fn list_devices(&self, input: ListDevicesInput) -> DomainResult<Vec<Device>> {
        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId("Organization ID cannot be empty".to_string()));
        }

        debug!(organization_id = %input.organization_id, "Listing devices");

        let devices = self.repository.list_devices(input).await?;

        info!(count = devices.len(), "Listed devices");
        Ok(devices)
    }
}
```

#### 6. Library Exports

**File**: `crates/ponix-domain/src/lib.rs`
**Changes**: Public API exports

```rust
pub mod error;
pub mod repository;
pub mod service;
pub mod types;

pub use error::{DomainError, DomainResult};
pub use repository::DeviceRepository;
pub use service::DeviceService;
pub use types::{CreateDeviceInput, Device, GetDeviceInput, ListDevicesInput};
```

#### 7. Unit Tests with Mockall

**File**: `crates/ponix-domain/src/service.rs` (tests module)
**Changes**: Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::MockDeviceRepository;

    #[tokio::test]
    async fn test_create_device_success() {
        let mut mock_repo = MockDeviceRepository::new();

        let expected_device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_repo
            .expect_create_device()
            .withf(|input: &CreateDeviceInput| {
                input.device_id == "device-123"
                    && input.organization_id == "org-456"
                    && input.name == "Test Device"
            })
            .times(1)
            .return_once(|_| Ok(expected_device.clone()));

        let service = DeviceService::new(Arc::new(mock_repo));

        let input = CreateDeviceInput {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
        };

        let result = service.create_device(input).await;
        assert!(result.is_ok());

        let device = result.unwrap();
        assert_eq!(device.device_id, "device-123");
        assert_eq!(device.name, "Test Device");
    }

    #[tokio::test]
    async fn test_create_device_empty_id() {
        let mock_repo = MockDeviceRepository::new();
        let service = DeviceService::new(Arc::new(mock_repo));

        let input = CreateDeviceInput {
            device_id: "".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
        };

        let result = service.create_device(input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DomainError::InvalidDeviceId(_)));
    }

    #[tokio::test]
    async fn test_get_device_success() {
        let mut mock_repo = MockDeviceRepository::new();

        let expected_device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_repo
            .expect_get_device()
            .withf(|input: &GetDeviceInput| input.device_id == "device-123")
            .times(1)
            .return_once(|_| Ok(Some(expected_device)));

        let service = DeviceService::new(Arc::new(mock_repo));

        let input = GetDeviceInput {
            device_id: "device-123".to_string(),
        };

        let result = service.get_device(input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_device_not_found() {
        let mut mock_repo = MockDeviceRepository::new();

        mock_repo
            .expect_get_device()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DeviceService::new(Arc::new(mock_repo));

        let input = GetDeviceInput {
            device_id: "nonexistent".to_string(),
        };

        let result = service.get_device(input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DomainError::DeviceNotFound(_)));
    }

    #[tokio::test]
    async fn test_list_devices_success() {
        let mut mock_repo = MockDeviceRepository::new();

        let devices = vec![
            Device {
                device_id: "device-1".to_string(),
                organization_id: "org-456".to_string(),
                name: "Device 1".to_string(),
                created_at: None,
                updated_at: None,
            },
            Device {
                device_id: "device-2".to_string(),
                organization_id: "org-456".to_string(),
                name: "Device 2".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        mock_repo
            .expect_list_devices()
            .withf(|input: &ListDevicesInput| input.organization_id == "org-456")
            .times(1)
            .return_once(|_| Ok(devices));

        let service = DeviceService::new(Arc::new(mock_repo));

        let input = ListDevicesInput {
            organization_id: "org-456".to_string(),
        };

        let result = service.list_devices(input).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }
}
```

#### 8. Update Workspace

**File**: `Cargo.toml`
**Changes**: Add ponix-domain to workspace members

```toml
[workspace]
members = [
    "crates/runner",
    "crates/ponix-nats",
    "crates/ponix-clickhouse",
    "crates/ponix-all-in-one",
    "crates/goose",
    "crates/ponix-postgres",
    "crates/ponix-domain",  # Add this line
]
```

### Success Criteria

#### Automated Verification:
- [x] Domain crate builds successfully: `cargo build -p ponix-domain`
- [x] All unit tests pass: `cargo test -p ponix-domain`
- [x] No linting errors: `cargo clippy -p ponix-domain`
- [x] Type checking passes: `cargo check -p ponix-domain`
- [x] Documentation builds: `cargo doc -p ponix-domain --no-deps`

#### Manual Verification:
- [ ] MockDeviceRepository can be instantiated in tests
- [ ] DeviceService business logic validation works as expected
- [ ] Domain error types provide clear error messages
- [ ] Repository trait can be implemented without circular dependencies

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation from the human that the manual testing was successful before proceeding to Phase 2.

---

## Phase 2: Repository Pattern Implementation

### Overview
Refactor `ponix-postgres` to implement the `DeviceRepository` trait from `ponix-domain`. Add type conversions between domain and database models, update existing device store operations to work through the trait.

### Changes Required

#### 1. Add ponix-domain Dependency

**File**: `crates/ponix-postgres/Cargo.toml`
**Changes**: Add domain dependency

```toml
[dependencies]
# ... existing dependencies ...

# Domain layer dependency
ponix-domain = { path = "../ponix-domain" }

# Add chrono for timestamp handling
chrono = { version = "0.4", features = ["serde"] }
```

#### 2. Domain to Database Type Conversions

**File**: `crates/ponix-postgres/src/conversions.rs`
**Changes**: Create new file for type mappings

```rust
use chrono::{DateTime, Utc};
use ponix_domain::{CreateDeviceInput, Device};
use crate::models::{Device as DbDevice, DeviceRow};

/// Convert domain Device to database Device (for insert)
impl From<&CreateDeviceInput> for DbDevice {
    fn from(input: &CreateDeviceInput) -> Self {
        DbDevice {
            device_id: input.device_id.clone(),
            organization_id: input.organization_id.clone(),
            device_name: input.name.clone(), // Map name -> device_name
        }
    }
}

/// Convert database DeviceRow to domain Device
impl From<DeviceRow> for Device {
    fn from(row: DeviceRow) -> Self {
        Device {
            device_id: row.device_id,
            organization_id: row.organization_id,
            name: row.device_name, // Map device_name -> name
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// Convert database Device (without timestamps) to domain Device
impl From<DbDevice> for Device {
    fn from(device: DbDevice) -> Self {
        Device {
            device_id: device.device_id,
            organization_id: device.organization_id,
            name: device.device_name, // Map device_name -> name
            created_at: None,
            updated_at: None,
        }
    }
}
```

#### 3. Implement DeviceRepository Trait

**File**: `crates/ponix-postgres/src/repository_impl.rs`
**Changes**: Create repository implementation

```rust
use async_trait::async_trait;
use chrono::Utc;
use tracing::debug;

use ponix_domain::{
    CreateDeviceInput, Device, DeviceRepository, DomainError, DomainResult,
    GetDeviceInput, ListDevicesInput,
};

use crate::client::PostgresClient;
use crate::models::DeviceRow;

/// PostgreSQL implementation of DeviceRepository trait
#[derive(Clone)]
pub struct PostgresDeviceRepository {
    client: PostgresClient,
}

impl PostgresDeviceRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl DeviceRepository for PostgresDeviceRepository {
    async fn create_device(&self, input: CreateDeviceInput) -> DomainResult<Device> {
        let conn = self.client.get_connection().await
            .map_err(|e| DomainError::RepositoryError(e))?;

        let now = Utc::now();

        // Execute insert
        let result = conn.execute(
            "INSERT INTO devices (device_id, organization_id, device_name, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &input.device_id,
                &input.organization_id,
                &input.name, // Map name -> device_name
                &now,
                &now,
            ],
        )
        .await;

        // Check for unique constraint violation
        if let Err(e) = result {
            let error_msg = e.to_string();
            if error_msg.contains("duplicate key") || error_msg.contains("unique constraint") {
                return Err(DomainError::DeviceAlreadyExists(input.device_id));
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!("Registered device: {}", input.device_id);

        // Return created device
        Ok(Device {
            device_id: input.device_id,
            organization_id: input.organization_id,
            name: input.name,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Option<Device>> {
        let conn = self.client.get_connection().await
            .map_err(|e| DomainError::RepositoryError(e))?;

        let row = conn
            .query_opt(
                "SELECT device_id, organization_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE device_id = $1",
                &[&input.device_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let device_row = DeviceRow {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    device_name: row.get(2),
                    created_at: row.get(3),
                    updated_at: row.get(4),
                };
                Ok(Some(device_row.into()))
            }
            None => Ok(None),
        }
    }

    async fn list_devices(&self, input: ListDevicesInput) -> DomainResult<Vec<Device>> {
        let conn = self.client.get_connection().await
            .map_err(|e| DomainError::RepositoryError(e))?;

        let rows = conn
            .query(
                "SELECT device_id, organization_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE organization_id = $1
                 ORDER BY created_at DESC",
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let devices = rows
            .iter()
            .map(|row| {
                let device_row = DeviceRow {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    device_name: row.get(2),
                    created_at: row.get(3),
                    updated_at: row.get(4),
                };
                device_row.into()
            })
            .collect();

        debug!(
            "Found {} devices for organization: {}",
            rows.len(),
            input.organization_id
        );

        Ok(devices)
    }
}
```

#### 4. Update Library Exports

**File**: `crates/ponix-postgres/src/lib.rs`
**Changes**: Export repository implementation

```rust
pub use client::PostgresClient;
pub use config::PostgresConfig;
pub use device_store::DeviceStore; // Keep for backward compatibility
pub use goose::MigrationRunner;
pub use models::{Device, DeviceRow};
pub use repository_impl::PostgresDeviceRepository; // Add this line

mod client;
mod config;
mod conversions; // Add this line
mod device_store;
mod models;
mod repository_impl; // Add this line
```

#### 5. Integration Tests for Repository

**File**: `crates/ponix-postgres/tests/repository_integration.rs`
**Changes**: Create integration test file

```rust
#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod tests {
    use ponix_domain::{CreateDeviceInput, DeviceRepository, GetDeviceInput, ListDevicesInput};
    use ponix_postgres::{PostgresClient, PostgresDeviceRepository, MigrationRunner};
    use testcontainers::{clients::Cli, RunnableImage};
    use testcontainers_modules::postgres::Postgres;

    async fn setup_test_db() -> (Cli, PostgresDeviceRepository) {
        let docker = Cli::default();
        let postgres = Postgres::default();
        let container = docker.run(postgres);

        let host = container.get_host().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();

        // Run migrations
        let dsn = format!("postgres://postgres:postgres@{}:{}/postgres?sslmode=disable", host, port);
        let migrations_dir = "../ponix-postgres/migrations".to_string();
        let goose_path = which::which("goose").expect("goose binary not found");

        let migration_runner = MigrationRunner::new(
            goose_path.to_string_lossy().to_string(),
            migrations_dir,
            "postgres".to_string(),
            dsn.clone(),
        );

        migration_runner.run_migrations().await.expect("Migrations failed");

        // Create client
        let client = PostgresClient::new(&host, port, "postgres", "postgres", "postgres", 5)
            .expect("Failed to create client");

        let repository = PostgresDeviceRepository::new(client);

        (docker, repository)
    }

    #[tokio::test]
    #[cfg_attr(not(feature = "integration-tests"), ignore)]
    async fn test_create_and_get_device() {
        let (_docker, repo) = setup_test_db().await;

        let input = CreateDeviceInput {
            device_id: "test-device-123".to_string(),
            organization_id: "test-org-456".to_string(),
            name: "Test Device".to_string(),
        };

        // Create device
        let created = repo.create_device(input.clone()).await.unwrap();
        assert_eq!(created.device_id, "test-device-123");
        assert_eq!(created.name, "Test Device");
        assert!(created.created_at.is_some());

        // Get device
        let get_input = GetDeviceInput {
            device_id: "test-device-123".to_string(),
        };
        let retrieved = repo.get_device(get_input).await.unwrap();
        assert!(retrieved.is_some());

        let device = retrieved.unwrap();
        assert_eq!(device.device_id, "test-device-123");
        assert_eq!(device.name, "Test Device");
    }

    #[tokio::test]
    #[cfg_attr(not(feature = "integration-tests"), ignore)]
    async fn test_list_devices_by_organization() {
        let (_docker, repo) = setup_test_db().await;

        // Create multiple devices
        for i in 1..=3 {
            let input = CreateDeviceInput {
                device_id: format!("device-{}", i),
                organization_id: "test-org".to_string(),
                name: format!("Device {}", i),
            };
            repo.create_device(input).await.unwrap();
        }

        // List devices
        let list_input = ListDevicesInput {
            organization_id: "test-org".to_string(),
        };
        let devices = repo.list_devices(list_input).await.unwrap();

        assert_eq!(devices.len(), 3);
        assert!(devices.iter().all(|d| d.organization_id == "test-org"));
    }

    #[tokio::test]
    #[cfg_attr(not(feature = "integration-tests"), ignore)]
    async fn test_create_duplicate_device() {
        let (_docker, repo) = setup_test_db().await;

        let input = CreateDeviceInput {
            device_id: "duplicate-device".to_string(),
            organization_id: "test-org".to_string(),
            name: "Original Device".to_string(),
        };

        // First creation should succeed
        repo.create_device(input.clone()).await.unwrap();

        // Second creation should fail with DeviceAlreadyExists
        let result = repo.create_device(input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ponix_domain::DomainError::DeviceAlreadyExists(_)
        ));
    }
}
```

#### 6. Add Integration Test Feature

**File**: `crates/ponix-postgres/Cargo.toml`
**Changes**: Add feature flag for integration tests

```toml
[features]
integration-tests = []

[dev-dependencies]
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres"] }
tokio-test = "0.4"
which = "7.0"
```

### Success Criteria

#### Automated Verification:
- [x] Postgres crate builds with domain dependency: `cargo build -p ponix-postgres`
- [x] Unit tests pass: `cargo test -p ponix-postgres --lib`
- [x] Integration tests pass: `cargo test -p ponix-postgres --features integration-tests -- --test-threads=1`
- [x] No linting errors: `cargo clippy -p ponix-postgres`
- [x] Type checking passes: `cargo check -p ponix-postgres`

#### Manual Verification:
- [ ] DeviceRepository trait is implemented correctly
- [ ] Type conversions handle field name mapping (device_name ↔ name)
- [ ] Error handling properly maps database errors to domain errors
- [ ] Duplicate key violations return DeviceAlreadyExists error

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation from the human that the manual testing was successful before proceeding to Phase 3.

---

## Phase 3: gRPC API Layer

### Overview
Create the `ponix-grpc` crate with Tonic-based gRPC server implementation. Pull the Tonic package from BSR for service definitions, implement handlers that map Proto ↔ Domain types, and handle error mapping to gRPC status codes.

### Changes Required

#### 1. Create ponix-grpc Crate Structure

**File**: `crates/ponix-grpc/Cargo.toml`
**Changes**: Create new crate manifest

```toml
[package]
name = "ponix-grpc"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[features]
default = []
device = ["ponix-proto-prost", "ponix-proto-tonic"]  # Feature flag for device service

[dependencies]
tokio = { workspace = true }
tokio-util = { workspace = true }
tonic = "0.12"
anyhow = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = "0.1"

# Domain layer
ponix-domain = { path = "../ponix-domain" }

# Protobuf types (prost)
ponix-proto-prost = { package = "ponix_ponix_community_neoeinstein-prost", version = "0.4.0-20251105022230-76aad410c133.1", registry = "buf", optional = true }

# Protobuf service definitions (tonic)
ponix-proto-tonic = { package = "ponix_ponix_community_neoeinstein-tonic", version = "0.4.0-20251105022230-76aad410c133.1", registry = "buf", optional = true }

prost-types = { workspace = true }
chrono = "0.4"

[dev-dependencies]
tokio-test = "0.4"
```

#### 2. Proto to Domain Type Conversions

**File**: `crates/ponix-grpc/src/conversions.rs`
**Changes**: Create type mapping utilities

```rust
use chrono::{DateTime, Utc};
use ponix_domain::{CreateDeviceInput, Device, GetDeviceInput, ListDevicesInput};
use ponix_proto_prost::end_device::v1::{
    CreateEndDeviceRequest, CreateEndDeviceResponse, EndDevice,
    GetEndDeviceRequest, GetEndDeviceResponse, ListEndDevicesRequest,
    ListEndDevicesResponse,
};
use prost_types::Timestamp;

/// Convert protobuf Timestamp to chrono DateTime
fn timestamp_to_datetime(ts: Option<Timestamp>) -> Option<DateTime<Utc>> {
    ts.and_then(|t| {
        DateTime::from_timestamp(t.seconds, t.nanos as u32)
    })
}

/// Convert chrono DateTime to protobuf Timestamp
fn datetime_to_timestamp(dt: Option<DateTime<Utc>>) -> Option<Timestamp> {
    dt.map(|d| Timestamp {
        seconds: d.timestamp(),
        nanos: d.timestamp_subsec_nanos() as i32,
    })
}

/// Convert protobuf CreateEndDeviceRequest to domain CreateDeviceInput
impl From<CreateEndDeviceRequest> for CreateDeviceInput {
    fn from(req: CreateEndDeviceRequest) -> Self {
        CreateDeviceInput {
            device_id: req.device_id,
            organization_id: req.organization_id,
            name: req.name,
        }
    }
}

/// Convert domain Device to protobuf EndDevice
impl From<Device> for EndDevice {
    fn from(device: Device) -> Self {
        EndDevice {
            device_id: device.device_id,
            organization_id: device.organization_id,
            name: device.name,
            created_at: datetime_to_timestamp(device.created_at),
            updated_at: datetime_to_timestamp(device.updated_at),
        }
    }
}

/// Convert protobuf GetEndDeviceRequest to domain GetDeviceInput
impl From<GetEndDeviceRequest> for GetDeviceInput {
    fn from(req: GetEndDeviceRequest) -> Self {
        GetDeviceInput {
            device_id: req.device_id,
        }
    }
}

/// Convert protobuf ListEndDevicesRequest to domain ListDevicesInput
impl From<ListEndDevicesRequest> for ListDevicesInput {
    fn from(req: ListEndDevicesRequest) -> Self {
        ListDevicesInput {
            organization_id: req.organization_id,
        }
    }
}
```

#### 3. Domain Error to gRPC Status Mapping

**File**: `crates/ponix-grpc/src/error.rs`
**Changes**: Map domain errors to gRPC codes

```rust
use ponix_domain::DomainError;
use tonic::{Code, Status};

/// Convert domain error to gRPC Status
pub fn domain_error_to_status(error: DomainError) -> Status {
    match error {
        DomainError::DeviceNotFound(msg) => Status::not_found(msg),

        DomainError::DeviceAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidDeviceId(msg)
        | DomainError::InvalidOrganizationId(msg)
        | DomainError::InvalidDeviceName(msg) => Status::invalid_argument(msg),

        DomainError::RepositoryError(err) => {
            Status::internal(format!("Internal error: {}", err))
        }
    }
}
```

#### 4. Device Service Handler

**File**: `crates/ponix-grpc/src/device_handler.rs`
**Changes**: Implement DeviceService gRPC handler

```rust
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info};

use ponix_domain::DeviceService;
use ponix_proto_prost::end_device::v1::{
    CreateEndDeviceRequest, CreateEndDeviceResponse, EndDevice,
    GetEndDeviceRequest, GetEndDeviceResponse, ListEndDevicesRequest,
    ListEndDevicesResponse,
};
use ponix_proto_tonic::end_device::v1::device_service_server::DeviceService as DeviceServiceTrait;

use crate::error::domain_error_to_status;

/// gRPC handler for DeviceService
/// Handles Proto → Domain mapping and error conversion
pub struct DeviceServiceHandler {
    domain_service: Arc<DeviceService>,
}

impl DeviceServiceHandler {
    pub fn new(domain_service: Arc<DeviceService>) -> Self {
        Self { domain_service }
    }
}

#[tonic::async_trait]
impl DeviceServiceTrait for DeviceServiceHandler {
    async fn create_device(
        &self,
        request: Request<CreateEndDeviceRequest>,
    ) -> Result<Response<CreateEndDeviceResponse>, Status> {
        let req = request.into_inner();

        debug!(
            device_id = %req.device_id,
            organization_id = %req.organization_id,
            "Received CreateDevice request"
        );

        // Convert proto → domain
        let input = req.into();

        // Call domain service
        let device = self.domain_service.create_device(input).await
            .map_err(domain_error_to_status)?;

        info!(device_id = %device.device_id, "Device created successfully");

        // Convert domain → proto
        let proto_device: EndDevice = device.into();

        Ok(Response::new(CreateEndDeviceResponse {
            device: Some(proto_device),
        }))
    }

    async fn get_device(
        &self,
        request: Request<GetEndDeviceRequest>,
    ) -> Result<Response<GetEndDeviceResponse>, Status> {
        let req = request.into_inner();

        debug!(device_id = %req.device_id, "Received GetDevice request");

        // Convert proto → domain
        let input = req.into();

        // Call domain service
        let device = self.domain_service.get_device(input).await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_device: EndDevice = device.into();

        Ok(Response::new(GetEndDeviceResponse {
            device: Some(proto_device),
        }))
    }

    async fn list_devices(
        &self,
        request: Request<ListEndDevicesRequest>,
    ) -> Result<Response<ListEndDevicesResponse>, Status> {
        let req = request.into_inner();

        debug!(organization_id = %req.organization_id, "Received ListDevices request");

        // Convert proto → domain
        let input = req.into();

        // Call domain service
        let devices = self.domain_service.list_devices(input).await
            .map_err(domain_error_to_status)?;

        info!(count = devices.len(), "Listed devices");

        // Convert domain → proto
        let proto_devices: Vec<EndDevice> = devices.into_iter().map(Into::into).collect();

        Ok(Response::new(ListEndDevicesResponse {
            devices: proto_devices,
        }))
    }
}
```

#### 5. gRPC Server

**File**: `crates/ponix-grpc/src/server.rs`
**Changes**: Implement Tonic server

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing::{info, error};

use ponix_domain::DeviceService;
use ponix_proto_tonic::end_device::v1::device_service_server::DeviceServiceServer;

use crate::device_handler::DeviceServiceHandler;

/// gRPC server configuration
pub struct GrpcServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 50051,
        }
    }
}

/// Run the gRPC server with graceful shutdown
pub async fn run_grpc_server(
    config: GrpcServerConfig,
    domain_service: Arc<DeviceService>,
    cancellation_token: CancellationToken,
) -> Result<(), anyhow::Error> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid server address");

    info!("Starting gRPC server on {}", addr);

    // Create handler
    let handler = DeviceServiceHandler::new(domain_service);

    // Build server with graceful shutdown
    let server = Server::builder()
        .add_service(DeviceServiceServer::new(handler))
        .serve_with_shutdown(addr, async move {
            cancellation_token.cancelled().await;
            info!("gRPC server shutdown signal received");
        });

    match server.await {
        Ok(_) => {
            info!("gRPC server stopped gracefully");
            Ok(())
        }
        Err(e) => {
            error!("gRPC server error: {}", e);
            Err(e.into())
        }
    }
}
```

#### 6. Library Exports

**File**: `crates/ponix-grpc/src/lib.rs`
**Changes**: Public API

```rust
#[cfg(feature = "device")]
pub mod conversions;
#[cfg(feature = "device")]
pub mod device_handler;
#[cfg(feature = "device")]
pub mod error;
#[cfg(feature = "device")]
pub mod server;

#[cfg(feature = "device")]
pub use device_handler::DeviceServiceHandler;
#[cfg(feature = "device")]
pub use server::{run_grpc_server, GrpcServerConfig};
```

#### 7. Unit Tests for Type Conversions

**File**: `crates/ponix-grpc/src/conversions.rs` (tests module)
**Changes**: Add conversion tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_create_request_to_domain() {
        let req = CreateEndDeviceRequest {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
        };

        let input: CreateDeviceInput = req.into();

        assert_eq!(input.device_id, "device-123");
        assert_eq!(input.organization_id, "org-456");
        assert_eq!(input.name, "Test Device");
    }

    #[test]
    fn test_domain_device_to_proto() {
        let now = Utc::now();
        let device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto: EndDevice = device.into();

        assert_eq!(proto.device_id, "device-123");
        assert_eq!(proto.organization_id, "org-456");
        assert_eq!(proto.name, "Test Device");
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());
    }
}
```

#### 8. Update Workspace

**File**: `Cargo.toml`
**Changes**: Add ponix-grpc to workspace

```toml
[workspace]
members = [
    "crates/runner",
    "crates/ponix-nats",
    "crates/ponix-clickhouse",
    "crates/ponix-all-in-one",
    "crates/goose",
    "crates/ponix-postgres",
    "crates/ponix-domain",
    "crates/ponix-grpc",  # Add this line
]
```

### Success Criteria

#### Automated Verification:
- [ ] gRPC crate builds successfully: `cargo build -p ponix-grpc --features device`
- [ ] All unit tests pass: `cargo test -p ponix-grpc --features device`
- [ ] No linting errors: `cargo clippy -p ponix-grpc --features device`
- [ ] Type checking passes: `cargo check -p ponix-grpc --features device`
- [ ] Documentation builds: `cargo doc -p ponix-grpc --features device --no-deps`

#### Manual Verification:
- [ ] Type conversions handle all fields correctly
- [ ] Error mapping produces appropriate gRPC status codes
- [ ] DeviceServiceHandler can be instantiated with domain service
- [ ] Server configuration accepts host and port

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation from the human that the manual testing was successful before proceeding to Phase 4.

---

## Phase 4: Service Integration

### Overview
Wire everything together in `ponix-all-in-one`: integrate gRPC server as a runner process, add configuration for gRPC settings, initialize the domain service and repository, and update Docker Compose to expose the gRPC port.

### Changes Required

#### 1. Add Dependencies

**File**: `crates/ponix-all-in-one/Cargo.toml`
**Changes**: Add new crate dependencies

```toml
[dependencies]
# ... existing dependencies ...

# New dependencies for gRPC device management
ponix-domain = { path = "../ponix-domain" }
ponix-grpc = { path = "../ponix-grpc", features = ["device"] }
```

#### 2. Extend Configuration

**File**: `crates/ponix-all-in-one/src/config.rs`
**Changes**: Add gRPC configuration fields

```rust
// Add to ServiceConfig struct
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceConfig {
    // ... existing fields ...

    // gRPC server configuration
    #[serde(default = "default_grpc_host")]
    pub grpc_host: String,

    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
}

// Add default functions
fn default_grpc_host() -> String {
    "0.0.0.0".to_string()
}

fn default_grpc_port() -> u16 {
    50051
}
```

#### 3. Update Service Initialization

**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Add gRPC server initialization

```rust
use std::sync::Arc;
use ponix_domain::{DeviceService, DeviceRepository};
use ponix_grpc::{run_grpc_server, GrpcServerConfig};
use ponix_postgres::PostgresDeviceRepository;

// Add after Phase 1 (PostgreSQL migrations) - around line 63
// PHASE 1.5: Initialize PostgreSQL client for device repository
info!("Connecting to PostgreSQL...");
let postgres_client = ponix_postgres::PostgresClient::new(
    &config.postgres_host,
    config.postgres_port,
    &config.postgres_database,
    &config.postgres_username,
    &config.postgres_password,
    config.postgres_max_pool_size,
)?;

if let Err(e) = postgres_client.ping().await {
    error!("Failed to ping PostgreSQL: {}", e);
    std::process::exit(1);
}
info!("PostgreSQL connection established");

// Create repository and domain service
let device_repository = PostgresDeviceRepository::new(postgres_client);
let device_service = Arc::new(DeviceService::new(Arc::new(device_repository)));

// Add to runner creation (around line 148) - before with_closer
let runner = Runner::new()
    .with_app_process({
        let config = config.clone();
        move |ctx| Box::pin(async move { run_producer_service(ctx, config, producer).await })
    })
    .with_app_process(move |ctx| Box::pin(async move { consumer.run(ctx).await }))
    // Add gRPC server process
    .with_app_process({
        let grpc_config = GrpcServerConfig {
            host: config.grpc_host.clone(),
            port: config.grpc_port,
        };
        let service = device_service.clone();
        move |ctx| {
            Box::pin(async move {
                run_grpc_server(grpc_config, service, ctx).await
            })
        }
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

#### 4. Update Docker Compose - Dependencies

**File**: `docker/docker-compose.deps.yaml`
**Changes**: Ensure PostgreSQL is included

```yaml
# This file should already have postgres from issue #16
# Verify it exists:
services:
  ponix-postgres:
    image: postgres:15-alpine
    container_name: ponix-postgres
    environment:
      POSTGRES_USER: ponix
      POSTGRES_PASSWORD: ponix
      POSTGRES_DB: ponix
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ponix"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  postgres_data:
```

#### 5. Update Docker Compose - Service

**File**: `docker/docker-compose.service.yaml`
**Changes**: Expose gRPC port and add environment variables

```yaml
services:
  ponix-all-in-one:
    # ... existing configuration ...

    ports:
      - "8080:8080"     # Existing HTTP port (if any)
      - "50051:50051"   # Add gRPC port

    environment:
      # ... existing env vars ...

      # gRPC configuration
      PONIX_GRPC_HOST: "0.0.0.0"
      PONIX_GRPC_PORT: "50051"
```

#### 6. Update .env.example

**File**: `.env.example`
**Changes**: Add gRPC configuration

```bash
# ... existing configuration ...

# gRPC Server Configuration
PONIX_GRPC_HOST=0.0.0.0
PONIX_GRPC_PORT=50051
```

#### 7. Update README

**File**: `README.md`
**Changes**: Document gRPC API usage

```markdown
## gRPC API

The service exposes a gRPC API for device management on port 50051 (configurable).

### Testing with grpcurl

```bash
# List services
grpcurl -plaintext localhost:50051 list

# Create a device
grpcurl -plaintext -d '{
  "device_id": "device-123",
  "organization_id": "org-456",
  "name": "My Device"
}' localhost:50051 ponix.end_device.v1.DeviceService/CreateDevice

# Get a device
grpcurl -plaintext -d '{
  "device_id": "device-123"
}' localhost:50051 ponix.end_device.v1.DeviceService/GetDevice

# List devices for an organization
grpcurl -plaintext -d '{
  "organization_id": "org-456"
}' localhost:50051 ponix.end_device.v1.DeviceService/ListDevices
```

### Configuration

gRPC server configuration via environment variables:
- `PONIX_GRPC_HOST` - Host to bind to (default: 0.0.0.0)
- `PONIX_GRPC_PORT` - Port to listen on (default: 50051)
```

### Success Criteria

#### Automated Verification:
- [ ] All workspace tests pass: `cargo test --workspace`
- [ ] Service builds successfully: `cargo build -p ponix-all-in-one`
- [ ] No linting errors: `cargo clippy --workspace`
- [ ] Type checking passes: `cargo check --workspace`
- [ ] Docker Compose builds: `docker-compose -f docker/docker-compose.service.yaml build`

#### Manual Verification:
- [ ] Service starts successfully with `docker-compose up`
- [ ] gRPC server accepts connections on port 50051
- [ ] Can successfully call CreateDevice via grpcurl
- [ ] Can successfully call GetDevice via grpcurl
- [ ] Can successfully call ListDevices via grpcurl
- [ ] Error responses return appropriate gRPC status codes (test with invalid inputs)
- [ ] Logs show all three processes running (producer, consumer, grpc server)
- [ ] Graceful shutdown works (Ctrl+C stops all processes cleanly)
- [ ] PostgreSQL migrations run on startup
- [ ] Devices are persisted correctly in PostgreSQL

**Implementation Note**: This is the final phase. After all automated and manual verification passes, the implementation is complete.

---

## Testing Strategy

### Unit Tests

**ponix-domain**:
- Test `DeviceService` business logic validation
- Test error handling for empty/invalid inputs
- Use mockall for `MockDeviceRepository`
- Verify service calls repository correctly

**ponix-grpc**:
- Test Proto → Domain type conversions
- Test Domain → Proto type conversions
- Test error mapping (DomainError → gRPC Status)
- Verify all fields are mapped correctly

### Integration Tests

**ponix-postgres**:
- Use testcontainers for PostgreSQL
- Test `DeviceRepository` implementation
- Test CRUD operations end-to-end
- Test unique constraint violations
- Test error handling for database failures

### End-to-End Testing (Manual)

**Using grpcurl**:
1. Start service: `docker-compose -f docker/docker-compose.service.yaml up`
2. Create device and verify response
3. Get device and verify data matches
4. List devices and verify all are returned
5. Test error cases:
   - Get non-existent device (should return NOT_FOUND)
   - Create duplicate device (should return ALREADY_EXISTS)
   - Create with empty fields (should return INVALID_ARGUMENT)

## Performance Considerations

- **Connection Pooling**: PostgreSQL uses deadpool with configurable pool size (default: 10)
- **Concurrent Requests**: Tonic server handles concurrent gRPC requests natively
- **Database Indexing**: Existing indexes on `organization_id` and `created_at` for efficient queries
- **Batch Operations**: Not implemented yet (can add later if needed)
- **Caching**: No caching layer (can add Redis later if needed)

## Migration Notes

**From Local Device Model to Protobuf**:
- Field name mapping: `device_name` (database) ↔ `name` (protobuf/domain)
- Handled in type conversions using `From`/`Into` traits
- Existing `DeviceStore` can remain for backward compatibility
- New code should use `DeviceRepository` trait

**Database Schema**:
- No schema changes required
- Migration `20251116000000_create_devices_table.sql` already exists
- Indexes already support efficient queries

## References

- Original ticket: Issue #17 - Implement gRPC API for Device Management
- Blocking dependency: ponix-protobuf#3 (Device protobuf definitions) - AVAILABLE in BSR
- Blocking dependency: ponix-rs#16 (PostgreSQL storage implementation) - IMPLEMENTED
- Related file: [device_store.rs](crates/ponix-postgres/src/device_store.rs) - Existing device storage
- Related file: [main.rs](crates/ponix-all-in-one/src/main.rs:42-165) - Service initialization pattern
- Related file: [runner/lib.rs](crates/runner/src/lib.rs) - Process lifecycle management
