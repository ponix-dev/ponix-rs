# Gateway CRUD Operations Implementation Plan

## Overview

Implement gateway management capabilities to allow organizations to register and manage their end device gateway configurations. This lays the groundwork for multi-tenant gateway support where each organization can configure their own gateway endpoints.

## Current State Analysis

The codebase follows Domain-Driven Design with clear separation of concerns:
- Domain layer defines business logic and repository traits
- Infrastructure layer (PostgreSQL) implements repository traits
- Services use xid for ID generation and handle validation
- All entities support soft delete via `deleted_at` field
- Configuration data is stored as TEXT columns with JSON serialization at the application layer

### Key Discoveries:
- Device and Organization implementations provide clear patterns to follow ([ponix-domain/src/end_device.rs](crates/ponix-domain/src/end_device.rs), [ponix-domain/src/organization.rs](crates/ponix-domain/src/organization.rs))
- Repository pattern uses `#[cfg_attr(test, mockall::automock)]` for automatic mocking ([ponix-domain/src/repository.rs:7-20](crates/ponix-domain/src/repository.rs#L7-L20))
- PostgreSQL uses TEXT columns for flexible data, not JSONB ([ponix-postgres/migrations](crates/ponix-postgres/migrations/))
- Services validate organization existence before creating related entities ([ponix-domain/src/end_device_service.rs:45-66](crates/ponix-domain/src/end_device_service.rs#L45-L66))
- Soft delete pattern with `deleted_at` timestamp and filtering in all queries

## Desired End State

After implementation:
- Organizations can create, read, update, and delete gateway configurations
- Each gateway has a type (e.g., "emqx") and type-specific JSON configuration
- Gateway operations respect organization boundaries and soft deletion
- Full integration with existing domain service patterns
- Comprehensive test coverage including unit and integration tests

### Verification:
- All gateway CRUD operations work via domain service
- PostgreSQL properly stores gateway configurations with JSONB support
- Organization relationship validation prevents orphaned gateways
- Soft delete functionality works correctly

## What We're NOT Doing

- Protobuf definitions for gateway types (separate ticket)
- gRPC service implementation (will be added after protobuf definitions)
- Dynamic process spawning when gateways are created (separate ticket)
- Actually connecting to gateways and consuming messages (separate ticket)
- Authentication/authorization for gateway connections (future work)

## Implementation Approach

Follow the existing patterns established by Device and Organization implementations:
1. Use two-type pattern for inputs (external without ID, internal with ID)
2. Generate IDs at the service layer using xid
3. Validate organization existence and non-deletion before creating gateways
4. Implement soft delete with `deleted_at` timestamp
5. Store gateway_config as JSONB column for flexible configuration schemas

## Phase 1: Database Schema & Migrations

### Overview
Create PostgreSQL table for gateways with proper indices and prepare for foreign key constraints.

### Changes Required:

#### 1. Create Gateways Table Migration
**File**: `crates/ponix-postgres/migrations/20251123170000_create_gateways_table.sql`
**Changes**: Create new migration file

```sql
-- +goose Up
-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS gateways (
    gateway_id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    gateway_type TEXT NOT NULL,
    gateway_config JSONB NOT NULL,
    deleted_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Index for querying gateways by organization
CREATE INDEX idx_gateways_organization_id ON gateways(organization_id);

-- Index for filtering by gateway type
CREATE INDEX idx_gateways_gateway_type ON gateways(gateway_type);

-- Index for soft delete filtering
CREATE INDEX idx_gateways_deleted_at ON gateways(deleted_at);

-- Index for sorting by creation time
CREATE INDEX idx_gateways_created_at ON gateways(created_at DESC);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP INDEX IF EXISTS idx_gateways_created_at;
DROP INDEX IF EXISTS idx_gateways_deleted_at;
DROP INDEX IF EXISTS idx_gateways_gateway_type;
DROP INDEX IF EXISTS idx_gateways_organization_id;
DROP TABLE IF EXISTS gateways;
-- +goose StatementEnd
```

#### 2. Add Foreign Key Constraint
**File**: `crates/ponix-postgres/migrations/20251123170001_add_gateway_organization_fk.sql`
**Changes**: Create new migration file

```sql
-- +goose Up
-- +goose StatementBegin
ALTER TABLE gateways
ADD CONSTRAINT fk_gateways_organization
FOREIGN KEY (organization_id)
REFERENCES organizations(id);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
ALTER TABLE gateways
DROP CONSTRAINT IF EXISTS fk_gateways_organization;
-- +goose StatementEnd
```

### Success Criteria:

#### Automated Verification:
- [x] Migrations apply cleanly: `cargo run --bin ponix-all-in-one` (runs migrations on startup)
- [x] No SQL syntax errors in migration files

#### Manual Verification:
- [ ] Table exists in PostgreSQL with correct schema
- [ ] Indices are created properly
- [ ] Foreign key constraint is enforced

---

## Phase 2: Domain Layer Implementation

### Overview
Implement Gateway entity, repository trait, input types, and domain service with business logic.

### Changes Required:

#### 1. Gateway Entity and Input Types
**File**: `crates/ponix-domain/src/gateway.rs`
**Changes**: Create new file

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Gateway entity representing a configured gateway for an organization
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gateway {
    pub gateway_id: String,
    pub organization_id: String,
    pub gateway_type: String,
    pub gateway_config: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// External input for creating a gateway (no ID)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateGatewayInput {
    pub organization_id: String,
    pub gateway_type: String,
    pub gateway_config: serde_json::Value,
}

/// Internal input with generated ID
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateGatewayInputWithId {
    pub gateway_id: String,
    pub organization_id: String,
    pub gateway_type: String,
    pub gateway_config: serde_json::Value,
}

/// Input for getting a gateway by ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetGatewayInput {
    pub gateway_id: String,
}

/// Input for updating a gateway
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateGatewayInput {
    pub gateway_id: String,
    pub gateway_type: Option<String>,
    pub gateway_config: Option<serde_json::Value>,
}

/// Input for deleting a gateway
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteGatewayInput {
    pub gateway_id: String,
}

/// Input for listing gateways by organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListGatewaysInput {
    pub organization_id: String,
}
```

#### 2. Update Repository Trait
**File**: `crates/ponix-domain/src/repository.rs`
**Changes**: Add GatewayRepository trait

```rust
use crate::gateway::{
    CreateGatewayInputWithId, DeleteGatewayInput, Gateway, GetGatewayInput, ListGatewaysInput,
    UpdateGatewayInput,
};

/// Repository trait for gateway persistence operations
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait GatewayRepository: Send + Sync {
    /// Create a new gateway
    async fn create_gateway(&self, input: CreateGatewayInputWithId) -> DomainResult<Gateway>;

    /// Get a gateway by ID (excludes soft deleted)
    async fn get_gateway(&self, input: GetGatewayInput) -> DomainResult<Option<Gateway>>;

    /// Update a gateway
    async fn update_gateway(&self, input: UpdateGatewayInput) -> DomainResult<Gateway>;

    /// Soft delete a gateway
    async fn delete_gateway(&self, input: DeleteGatewayInput) -> DomainResult<()>;

    /// List gateways by organization (excludes soft deleted)
    async fn list_gateways(&self, input: ListGatewaysInput) -> DomainResult<Vec<Gateway>>;
}
```

#### 3. Gateway Service
**File**: `crates/ponix-domain/src/gateway_service.rs`
**Changes**: Create new file

```rust
use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    error::{DomainError, DomainResult},
    gateway::{
        CreateGatewayInput, CreateGatewayInputWithId, DeleteGatewayInput, Gateway,
        GetGatewayInput, ListGatewaysInput, UpdateGatewayInput,
    },
    organization::{GetOrganizationInput, OrganizationRepository},
    repository::GatewayRepository,
};

/// Service for gateway business logic
pub struct GatewayService {
    gateway_repository: Arc<dyn GatewayRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
}

impl GatewayService {
    pub fn new(
        gateway_repository: Arc<dyn GatewayRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
    ) -> Self {
        Self {
            gateway_repository,
            organization_repository,
        }
    }

    /// Create a new gateway for an organization
    pub async fn create_gateway(&self, input: CreateGatewayInput) -> DomainResult<Gateway> {
        debug!(organization_id = %input.organization_id, gateway_type = %input.gateway_type, "Creating gateway");

        // Validate organization exists and is not deleted
        let org_input = GetOrganizationInput {
            organization_id: input.organization_id.clone(),
        };

        match self.organization_repository.get_organization(org_input).await? {
            Some(org) => {
                if org.deleted_at.is_some() {
                    return Err(DomainError::OrganizationDeleted(format!(
                        "Cannot create gateway for deleted organization: {}",
                        input.organization_id
                    )));
                }
            }
            None => {
                return Err(DomainError::OrganizationNotFound(format!(
                    "Organization not found: {}",
                    input.organization_id
                )));
            }
        }

        // Validate gateway_type is not empty
        if input.gateway_type.trim().is_empty() {
            return Err(DomainError::InvalidGatewayType(
                "Gateway type cannot be empty".to_string(),
            ));
        }

        // Generate gateway ID
        let gateway_id = xid::new().to_string();

        let repo_input = CreateGatewayInputWithId {
            gateway_id: gateway_id.clone(),
            organization_id: input.organization_id,
            gateway_type: input.gateway_type,
            gateway_config: input.gateway_config,
        };

        let gateway = self.gateway_repository.create_gateway(repo_input).await?;

        info!(gateway_id = %gateway.gateway_id, "Gateway created successfully");
        Ok(gateway)
    }

    /// Get a gateway by ID
    pub async fn get_gateway(&self, input: GetGatewayInput) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "Getting gateway");

        if input.gateway_id.is_empty() {
            return Err(DomainError::InvalidGatewayId(
                "Gateway ID cannot be empty".to_string(),
            ));
        }

        let gateway = self
            .gateway_repository
            .get_gateway(input.clone())
            .await?
            .ok_or_else(|| DomainError::GatewayNotFound(input.gateway_id.clone()))?;

        Ok(gateway)
    }

    /// Update a gateway
    pub async fn update_gateway(&self, input: UpdateGatewayInput) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "Updating gateway");

        if input.gateway_id.is_empty() {
            return Err(DomainError::InvalidGatewayId(
                "Gateway ID cannot be empty".to_string(),
            ));
        }

        // Validate gateway_type if provided
        if let Some(ref gateway_type) = input.gateway_type {
            if gateway_type.trim().is_empty() {
                return Err(DomainError::InvalidGatewayType(
                    "Gateway type cannot be empty".to_string(),
                ));
            }
        }

        let gateway = self.gateway_repository.update_gateway(input).await?;

        info!(gateway_id = %gateway.gateway_id, "Gateway updated successfully");
        Ok(gateway)
    }

    /// Soft delete a gateway
    pub async fn delete_gateway(&self, input: DeleteGatewayInput) -> DomainResult<()> {
        debug!(gateway_id = %input.gateway_id, "Deleting gateway");

        if input.gateway_id.is_empty() {
            return Err(DomainError::InvalidGatewayId(
                "Gateway ID cannot be empty".to_string(),
            ));
        }

        self.gateway_repository.delete_gateway(input).await?;

        info!("Gateway soft deleted successfully");
        Ok(())
    }

    /// List gateways by organization
    pub async fn list_gateways(&self, input: ListGatewaysInput) -> DomainResult<Vec<Gateway>> {
        debug!(organization_id = %input.organization_id, "Listing gateways");

        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId(
                "Organization ID cannot be empty".to_string(),
            ));
        }

        let gateways = self.gateway_repository.list_gateways(input).await?;

        info!(count = gateways.len(), "Listed gateways");
        Ok(gateways)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organization::MockOrganizationRepository;
    use crate::repository::MockGatewayRepository;
    use chrono::Utc;

    #[tokio::test]
    async fn test_create_gateway_success() {
        let mut mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        let test_config = serde_json::json!({
            "host": "mqtt.example.com",
            "port": 1883
        });

        mock_org_repo
            .expect_get_organization()
            .withf(|input| input.organization_id == "org-001")
            .times(1)
            .return_once(|_| {
                Ok(Some(crate::organization::Organization {
                    id: "org-001".to_string(),
                    name: "Test Org".to_string(),
                    deleted_at: None,
                    created_at: Some(Utc::now()),
                    updated_at: Some(Utc::now()),
                }))
            });

        mock_gateway_repo
            .expect_create_gateway()
            .withf(|input| {
                input.organization_id == "org-001"
                    && input.gateway_type == "emqx"
                    && !input.gateway_id.is_empty()
            })
            .times(1)
            .return_once(|input| {
                Ok(Gateway {
                    gateway_id: input.gateway_id,
                    organization_id: input.organization_id,
                    gateway_type: input.gateway_type,
                    gateway_config: input.gateway_config,
                    deleted_at: None,
                    created_at: Some(Utc::now()),
                    updated_at: Some(Utc::now()),
                })
            });

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
        );

        let input = CreateGatewayInput {
            organization_id: "org-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: test_config,
        };

        let result = service.create_gateway(input).await;
        assert!(result.is_ok());
        let gateway = result.unwrap();
        assert_eq!(gateway.organization_id, "org-001");
        assert_eq!(gateway.gateway_type, "emqx");
    }

    #[tokio::test]
    async fn test_create_gateway_org_not_found() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(None));

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
        );

        let input = CreateGatewayInput {
            organization_id: "org-999".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: serde_json::json!({}),
        };

        let result = service.create_gateway(input).await;
        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_create_gateway_org_deleted() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| {
                Ok(Some(crate::organization::Organization {
                    id: "org-001".to_string(),
                    name: "Test Org".to_string(),
                    deleted_at: Some(Utc::now()),
                    created_at: Some(Utc::now()),
                    updated_at: Some(Utc::now()),
                }))
            });

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
        );

        let input = CreateGatewayInput {
            organization_id: "org-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: serde_json::json!({}),
        };

        let result = service.create_gateway(input).await;
        assert!(matches!(result, Err(DomainError::OrganizationDeleted(_))));
    }
}
```

#### 4. Update Domain Error Types
**File**: `crates/ponix-domain/src/error.rs`
**Changes**: Add gateway-related error variants

```rust
// Add these variants to the DomainError enum
#[error("Gateway not found: {0}")]
GatewayNotFound(String),

#[error("Gateway already exists: {0}")]
GatewayAlreadyExists(String),

#[error("Invalid gateway ID: {0}")]
InvalidGatewayId(String),

#[error("Invalid gateway type: {0}")]
InvalidGatewayType(String),

#[error("Invalid gateway configuration: {0}")]
InvalidGatewayConfig(String),
```

#### 5. Update Module Exports
**File**: `crates/ponix-domain/src/lib.rs`
**Changes**: Export gateway module

```rust
pub mod gateway;
pub mod gateway_service;

pub use gateway::{
    CreateGatewayInput, CreateGatewayInputWithId, DeleteGatewayInput, Gateway, GetGatewayInput,
    ListGatewaysInput, UpdateGatewayInput,
};
pub use gateway_service::GatewayService;
```

### Success Criteria:

#### Automated Verification:
- [ ] Domain crate compiles: `cargo build -p ponix-domain`
- [ ] Domain tests pass: `cargo test -p ponix-domain`
- [ ] No clippy warnings: `cargo clippy -p ponix-domain`

#### Manual Verification:
- [ ] Service properly validates inputs
- [ ] Organization existence check works correctly
- [ ] ID generation produces valid xids

---

## Phase 3: PostgreSQL Repository Implementation

### Overview
Implement the GatewayRepository trait with PostgreSQL database operations.

### Changes Required:

#### 1. Database Models
**File**: `crates/ponix-postgres/src/models.rs`
**Changes**: Add Gateway models

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRow {
    pub gateway_id: String,
    pub organization_id: String,
    pub gateway_type: String,
    pub gateway_config: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### 2. Type Conversions
**File**: `crates/ponix-postgres/src/conversions.rs`
**Changes**: Add Gateway conversions

```rust
use ponix_domain::gateway::{CreateGatewayInputWithId, Gateway};
use crate::models::GatewayRow;

impl From<GatewayRow> for Gateway {
    fn from(row: GatewayRow) -> Self {
        Gateway {
            gateway_id: row.gateway_id,
            organization_id: row.organization_id,
            gateway_type: row.gateway_type,
            gateway_config: row.gateway_config,
            deleted_at: row.deleted_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}
```

#### 3. Gateway Repository Implementation
**File**: `crates/ponix-postgres/src/gateway_repository.rs`
**Changes**: Create new file

```rust
use async_trait::async_trait;
use chrono::Utc;
use ponix_domain::{
    error::{DomainError, DomainResult},
    gateway::{
        CreateGatewayInputWithId, DeleteGatewayInput, Gateway, GetGatewayInput,
        ListGatewaysInput, UpdateGatewayInput,
    },
    repository::GatewayRepository,
};
use tracing::{debug, info};

use crate::{client::PostgresClient, models::GatewayRow};

#[derive(Clone)]
pub struct PostgresGatewayRepository {
    client: PostgresClient,
}

impl PostgresGatewayRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl GatewayRepository for PostgresGatewayRepository {
    async fn create_gateway(&self, input: CreateGatewayInputWithId) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "Creating gateway in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let result = conn
            .execute(
                "INSERT INTO gateways (gateway_id, organization_id, gateway_type, gateway_config, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &input.gateway_id,
                    &input.organization_id,
                    &input.gateway_type,
                    &input.gateway_config,
                    &now,
                    &now,
                ],
            )
            .await;

        if let Err(e) = result {
            if let Some(db_err) = e.as_db_error() {
                if db_err.code().code() == "23505" {
                    return Err(DomainError::GatewayAlreadyExists(input.gateway_id));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        info!(gateway_id = %input.gateway_id, "Gateway created in database");

        Ok(Gateway {
            gateway_id: input.gateway_id,
            organization_id: input.organization_id,
            gateway_type: input.gateway_type,
            gateway_config: input.gateway_config,
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    async fn get_gateway(&self, input: GetGatewayInput) -> DomainResult<Option<Gateway>> {
        debug!(gateway_id = %input.gateway_id, "Getting gateway from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT gateway_id, organization_id, gateway_type, gateway_config, deleted_at, created_at, updated_at
                 FROM gateways
                 WHERE gateway_id = $1 AND deleted_at IS NULL",
                &[&input.gateway_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let gateway = row.map(|row| {
            let gateway_row = GatewayRow {
                gateway_id: row.get(0),
                organization_id: row.get(1),
                gateway_type: row.get(2),
                gateway_config: row.get(3),
                deleted_at: row.get(4),
                created_at: row.get(5),
                updated_at: row.get(6),
            };
            gateway_row.into()
        });

        Ok(gateway)
    }

    async fn update_gateway(&self, input: UpdateGatewayInput) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "Updating gateway in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Build dynamic UPDATE query based on provided fields
        let mut query = String::from("UPDATE gateways SET updated_at = $1");
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&now];
        let mut param_idx = 2;

        if let Some(ref gateway_type) = input.gateway_type {
            query.push_str(&format!(", gateway_type = ${}", param_idx));
            params.push(gateway_type);
            param_idx += 1;
        }

        if let Some(ref gateway_config) = input.gateway_config {
            query.push_str(&format!(", gateway_config = ${}", param_idx));
            params.push(gateway_config);
            param_idx += 1;
        }

        query.push_str(&format!(
            " WHERE gateway_id = ${} AND deleted_at IS NULL
             RETURNING gateway_id, organization_id, gateway_type, gateway_config, deleted_at, created_at, updated_at",
            param_idx
        ));
        params.push(&input.gateway_id);

        let row = conn
            .query_opt(&query, &params[..])
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let gateway_row = GatewayRow {
                    gateway_id: row.get(0),
                    organization_id: row.get(1),
                    gateway_type: row.get(2),
                    gateway_config: row.get(3),
                    deleted_at: row.get(4),
                    created_at: row.get(5),
                    updated_at: row.get(6),
                };
                info!(gateway_id = %gateway_row.gateway_id, "Gateway updated in database");
                Ok(gateway_row.into())
            }
            None => Err(DomainError::GatewayNotFound(input.gateway_id)),
        }
    }

    async fn delete_gateway(&self, input: DeleteGatewayInput) -> DomainResult<()> {
        debug!(gateway_id = %input.gateway_id, "Soft deleting gateway");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let rows_affected = conn
            .execute(
                "UPDATE gateways
                 SET deleted_at = $1, updated_at = $1
                 WHERE gateway_id = $2 AND deleted_at IS NULL",
                &[&now, &input.gateway_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        if rows_affected == 0 {
            return Err(DomainError::GatewayNotFound(input.gateway_id));
        }

        info!(gateway_id = %input.gateway_id, "Gateway soft deleted");
        Ok(())
    }

    async fn list_gateways(&self, input: ListGatewaysInput) -> DomainResult<Vec<Gateway>> {
        debug!(organization_id = %input.organization_id, "Listing gateways from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT gateway_id, organization_id, gateway_type, gateway_config, deleted_at, created_at, updated_at
                 FROM gateways
                 WHERE organization_id = $1 AND deleted_at IS NULL
                 ORDER BY created_at DESC",
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let gateways: Vec<Gateway> = rows
            .into_iter()
            .map(|row| {
                let gateway_row = GatewayRow {
                    gateway_id: row.get(0),
                    organization_id: row.get(1),
                    gateway_type: row.get(2),
                    gateway_config: row.get(3),
                    deleted_at: row.get(4),
                    created_at: row.get(5),
                    updated_at: row.get(6),
                };
                gateway_row.into()
            })
            .collect();

        info!(count = gateways.len(), "Listed gateways from database");
        Ok(gateways)
    }
}
```

#### 4. Update Module Exports
**File**: `crates/ponix-postgres/src/lib.rs`
**Changes**: Export gateway repository

```rust
pub mod gateway_repository;

pub use gateway_repository::PostgresGatewayRepository;
```

### Success Criteria:

#### Automated Verification:
- [ ] PostgreSQL crate compiles: `cargo build -p ponix-postgres`
- [ ] No clippy warnings: `cargo clippy -p ponix-postgres`
- [ ] Type checking passes: `cargo check -p ponix-postgres`

#### Manual Verification:
- [ ] Repository correctly handles JSONB serialization
- [ ] Soft delete queries work properly
- [ ] Foreign key constraints are enforced

---

## Phase 4: Integration Testing

### Overview
Add comprehensive integration tests to verify the complete gateway CRUD functionality.

### Changes Required:

#### 1. Integration Tests for Gateway Repository
**File**: `crates/ponix-postgres/tests/gateway_repository_test.rs`
**Changes**: Create new test file

```rust
#[cfg(feature = "integration-tests")]
mod tests {
    use chrono::Utc;
    use ponix_domain::{
        gateway::{
            CreateGatewayInputWithId, DeleteGatewayInput, GetGatewayInput, ListGatewaysInput,
            UpdateGatewayInput,
        },
        organization::{CreateOrganizationInputWithId, OrganizationRepository},
        repository::GatewayRepository,
    };
    use ponix_postgres::{
        PostgresClient, PostgresConfig, PostgresGatewayRepository, PostgresOrganizationRepository,
    };
    use testcontainers::{clients::Cli, images::postgres::Postgres};

    async fn setup_test_db() -> (PostgresClient, Cli, testcontainers::Container<'_, Postgres>) {
        let docker = Cli::default();
        let container = docker.run(Postgres::default());
        let port = container.get_host_port_ipv4(5432);

        let config = PostgresConfig {
            host: "localhost".to_string(),
            port,
            database: "postgres".to_string(),
            username: "postgres".to_string(),
            password: "postgres".to_string(),
            max_pool_size: 5,
            migrations_dir: "crates/ponix-postgres/migrations".to_string(),
            goose_binary_path: "goose".to_string(),
        };

        let client = PostgresClient::new(&config).await.unwrap();

        // Run migrations
        let runner = goose::MigrationRunner::new(
            config.goose_binary_path.clone(),
            config.migrations_dir.clone(),
            "postgres".to_string(),
            client.get_dsn(),
        );
        runner.run_migrations().await.unwrap();

        (client, docker, container)
    }

    #[tokio::test]
    async fn test_gateway_crud_operations() {
        let (client, _docker, _container) = setup_test_db().await;

        // Create organization first
        let org_repo = PostgresOrganizationRepository::new(client.clone());
        let org_input = CreateOrganizationInputWithId {
            id: "org-test-001".to_string(),
            name: "Test Organization".to_string(),
        };
        org_repo.create_organization(org_input).await.unwrap();

        // Test gateway repository
        let gateway_repo = PostgresGatewayRepository::new(client.clone());

        // Test Create
        let create_input = CreateGatewayInputWithId {
            gateway_id: "gw-test-001".to_string(),
            organization_id: "org-test-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: serde_json::json!({
                "host": "mqtt.example.com",
                "port": 1883,
                "topic_pattern": "{organization}/{device}"
            }),
        };

        let created = gateway_repo.create_gateway(create_input).await.unwrap();
        assert_eq!(created.gateway_id, "gw-test-001");
        assert_eq!(created.gateway_type, "emqx");
        assert!(created.created_at.is_some());

        // Test Get
        let get_input = GetGatewayInput {
            gateway_id: "gw-test-001".to_string(),
        };
        let retrieved = gateway_repo.get_gateway(get_input).await.unwrap();
        assert!(retrieved.is_some());
        let gateway = retrieved.unwrap();
        assert_eq!(gateway.gateway_id, "gw-test-001");

        // Test Update
        let update_input = UpdateGatewayInput {
            gateway_id: "gw-test-001".to_string(),
            gateway_type: None,
            gateway_config: Some(serde_json::json!({
                "host": "mqtt2.example.com",
                "port": 8883
            })),
        };
        let updated = gateway_repo.update_gateway(update_input).await.unwrap();
        assert_eq!(updated.gateway_config["host"], "mqtt2.example.com");

        // Test List
        let list_input = ListGatewaysInput {
            organization_id: "org-test-001".to_string(),
        };
        let gateways = gateway_repo.list_gateways(list_input).await.unwrap();
        assert_eq!(gateways.len(), 1);

        // Test Delete
        let delete_input = DeleteGatewayInput {
            gateway_id: "gw-test-001".to_string(),
        };
        gateway_repo.delete_gateway(delete_input).await.unwrap();

        // Verify soft delete
        let get_input = GetGatewayInput {
            gateway_id: "gw-test-001".to_string(),
        };
        let deleted = gateway_repo.get_gateway(get_input).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_gateway_unique_constraint() {
        let (client, _docker, _container) = setup_test_db().await;

        let org_repo = PostgresOrganizationRepository::new(client.clone());
        let org_input = CreateOrganizationInputWithId {
            id: "org-test-002".to_string(),
            name: "Test Organization 2".to_string(),
        };
        org_repo.create_organization(org_input).await.unwrap();

        let gateway_repo = PostgresGatewayRepository::new(client.clone());

        let create_input = CreateGatewayInputWithId {
            gateway_id: "gw-test-002".to_string(),
            organization_id: "org-test-002".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: serde_json::json!({}),
        };

        gateway_repo.create_gateway(create_input.clone()).await.unwrap();

        // Try to create with same ID
        let result = gateway_repo.create_gateway(create_input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ponix_domain::error::DomainError::GatewayAlreadyExists(_)
        ));
    }
}
```

### Success Criteria:

#### Automated Verification:
- [ ] Integration tests pass: `cargo test --features integration-tests -p ponix-postgres -- gateway`
- [ ] All CRUD operations work correctly
- [ ] Soft delete functionality verified
- [ ] Unique constraint enforcement verified

#### Manual Verification:
- [ ] Tests run successfully with testcontainers
- [ ] Database schema is correct after migrations
- [ ] JSONB data is properly stored and retrieved

---

## Testing Strategy

### Unit Tests:
- Gateway service business logic validation
- Organization existence checks
- Input validation for all operations
- Mock repository interactions

### Integration Tests:
- Full CRUD cycle with real PostgreSQL
- Foreign key constraint validation
- Soft delete behavior
- JSONB serialization/deserialization
- Concurrent operations handling

### Manual Testing Steps:
1. Start PostgreSQL: `docker-compose -f docker/docker-compose.deps.yaml up -d`
2. Run migrations: `cargo run --bin ponix-all-in-one`
3. Connect to database: `docker exec -it ponix-postgres psql -U ponix -d ponix`
4. Verify schema: `\d gateways`
5. Test foreign key: Try inserting gateway with non-existent organization

## Performance Considerations

- Indexes on `organization_id`, `gateway_type`, `deleted_at`, and `created_at` for query performance
- JSONB column allows efficient querying of configuration data
- Soft delete pattern avoids data loss while maintaining referential integrity
- Connection pooling via `deadpool-postgres` for concurrent operations

## Migration Notes

- Migrations run automatically on service startup via `MigrationRunner`
- Foreign key constraint added in separate migration to ensure proper ordering
- JSONB column requires PostgreSQL 9.4+ (already using latest version)
- Rollback supported via goose's Down migrations

## References

- Original ticket: [GitHub Issue #40](https://github.com/ponix-dev/ponix-rs/issues/40)
- Device implementation: [ponix-domain/src/end_device.rs](crates/ponix-domain/src/end_device.rs)
- Organization implementation: [ponix-domain/src/organization.rs](crates/ponix-domain/src/organization.rs)
- Repository pattern: [ponix-domain/src/repository.rs](crates/ponix-domain/src/repository.rs)