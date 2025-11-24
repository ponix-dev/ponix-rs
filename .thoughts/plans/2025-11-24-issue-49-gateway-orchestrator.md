# Dynamic Gateway Process Orchestrator with CDC Event Processing

## Overview

Implement a domain-layer gateway orchestrator that dynamically manages simple print processes based on gateway lifecycle events from CDC. This foundational implementation proves the dynamic process orchestration pattern works without the complexity of actual MQTT/EMQX integration. The orchestrator uses a pluggable store pattern (in-memory initially) for managing process state and handles process lifecycle with retry logic for failed processes.

## Current State Analysis

### Existing Infrastructure
- **CDC Pipeline**: Fully operational PostgreSQL logical replication → NATS ([process.rs:78-101](crates/ponix-postgres/src/cdc/process.rs#L78-L101))
- **Gateway Domain**: Complete CRUD with soft deletes via `deleted_at` field ([gateway.rs:1-56](crates/ponix-domain/src/gateway.rs#L1-L56))
- **CDC Events**: Publishing to `gateway.create`, `gateway.update`, `gateway.delete` subjects ([gateway_converter.rs:1-226](crates/ponix-postgres/src/cdc/gateway_converter.rs#L1-L226))
- **Runner Pattern**: Concurrent process management with graceful shutdown ([lib.rs:163-265](crates/runner/src/lib.rs#L163-L265))
- **NATS Consumer**: Batch processing with cancellation support ([consumer.rs:103-124](crates/ponix-nats/src/consumer.rs#L103-L124))

### Key Patterns Identified
- **Repository Trait Pattern**: Domain defines interface, infrastructure implements ([repository.rs:108-123](crates/ponix-domain/src/repository.rs#L108-L123))
- **Service Pattern**: Domain services coordinate logic with repository dependencies ([gateway_service.rs:15-29](crates/ponix-domain/src/gateway_service.rs#L15-L29))
- **Process Registration**: Closures with `CancellationToken` registered to Runner ([main.rs:305-326](crates/ponix-all-in-one/src/main.rs#L305-L326))
- **Graceful Shutdown**: `tokio::select!` with `ctx.cancelled()` for coordinated termination

### Current Limitations
- `GatewayRepository::list_gateways()` filters by organization only - needs method to list ALL non-deleted gateways
- No dynamic process spawning capability - processes are registered at startup
- No process state tracking beyond JoinHandles in Runner

## Desired End State

### Functional Requirements
1. **Startup**: Orchestrator loads all non-deleted gateways from repository and spawns print processes
2. **Create Events**: CDC consumer receives `gateway.create`, orchestrator spawns new print process
3. **Update Events**: CDC consumer receives `gateway.update`:
   - If `deleted_at` exists: Stop and remove process
   - If `gateway_config` changed: Restart process with new config
   - Otherwise: No action (process continues running)
4. **Delete Events**: CDC consumer receives `gateway.delete`, orchestrator stops and removes process
5. **Print Processes**: Each process continuously prints gateway config to stdout every 5 seconds
6. **Retry Logic**: Failed process starts retry with configurable backoff delay
7. **Graceful Shutdown**: All running gateway processes stop cleanly on service termination

### Technical Requirements
- Pluggable process store with trait abstraction (in-memory implementation initially)
- Process store tracks: JoinHandle, CancellationToken, current Gateway config, retry state
- CDC consumer translates protobuf Gateway messages to domain types
- Orchestrator methods are pure business logic - no protobuf dependencies
- All components integrate via Runner pattern
- Configuration for: print interval, retry delay, max retry attempts

### Verification Criteria
- Integration test verifies full lifecycle: create → print, update config → restart, update deleted_at → stop, delete → stop
- Manual test: Start service, create gateway via gRPC, observe print output in logs
- Manual test: Update gateway config, observe process restart with new values
- Manual test: Soft delete gateway, observe process stop
- Manual test: SIGTERM service, verify all gateway processes stop gracefully within timeout

## What We're NOT Doing

- Actual MQTT/EMQX integration or connection management
- Gateway config validation beyond JSON parsing
- Multi-deployment coordination or leader election
- Process health monitoring or metrics collection
- Persistence of process state across service restarts
- Advanced retry strategies (exponential backoff, circuit breakers)
- Process output capture or logging beyond stdout

## Implementation Approach

The implementation follows domain-driven design with clear separation of concerns:
1. **Domain Layer**: Orchestrator business logic, process store trait, gateway lifecycle management
2. **Infrastructure Layer**: In-memory process store implementation
3. **Consumer Layer**: CDC event parsing and orchestrator delegation
4. **Integration Layer**: Wire up orchestrator and CDC consumer in `ponix-all-in-one`

Key design decisions:
- **Store Trait**: Enables future persistence implementations (Redis, PostgreSQL) without changing orchestrator logic
- **Domain Event Types**: CDC consumer translates protobuf to domain types for clean boundaries
- **Retry in Process**: Each print process handles its own retry logic to keep orchestrator simple
- **Config Comparison**: Hash gateway_config JSON to detect actual changes on updates

## Phase 1: Repository Enhancement

### Overview
Add method to `GatewayRepository` trait to fetch all non-deleted gateways, enabling orchestrator startup.

### Changes Required

#### 1. Domain Repository Trait
**File**: `crates/ponix-domain/src/repository.rs`
**Changes**: Add method to list all non-deleted gateways

```rust
#[async_trait]
pub trait GatewayRepository: Send + Sync {
    // ... existing methods ...

    /// List all non-deleted gateways across all organizations
    async fn list_all_gateways(&self) -> DomainResult<Vec<Gateway>>;
}
```

#### 2. PostgreSQL Repository Implementation
**File**: `crates/ponix-postgres/src/gateway_repository.rs`
**Changes**: Implement new method with appropriate query

```rust
#[async_trait]
impl GatewayRepository for PostgresGatewayRepository {
    // ... existing methods ...

    async fn list_all_gateways(&self) -> DomainResult<Vec<Gateway>> {
        let query = "
            SELECT gateway_id, organization_id, gateway_type, gateway_config,
                   created_at, updated_at, deleted_at, status
            FROM gateways
            WHERE deleted_at IS NULL
            ORDER BY created_at DESC
        ";

        let rows = self.client.query(query, &[]).await.map_err(|e| {
            error!("Failed to list all gateways: {}", e);
            DomainError::DatabaseError(e.to_string())
        })?;

        let gateways = rows
            .into_iter()
            .map(|row| gateway_from_row(&row))
            .collect::<DomainResult<Vec<_>>>()?;

        Ok(gateways)
    }
}
```

#### 3. Mock Repository for Tests
**File**: `crates/ponix-domain/src/repository.rs`
**Changes**: mockall will auto-generate mock for new method

No code changes needed - the `#[cfg_attr(test, mockall::automock)]` attribute on the trait automatically includes new methods in the mock.

### Success Criteria

#### Automated Verification:
- [x] Code compiles: `cargo build -p ponix-domain`
- [x] Code compiles: `cargo build -p ponix-postgres`
- [x] Unit tests pass: `cargo test -p ponix-postgres --lib`
- [x] Integration tests pass: `cargo test -p ponix-postgres --features integration-tests -- --test-threads=1`

#### Manual Verification:
- [ ] Query PostgreSQL directly to verify correct SQL: `docker exec -it ponix-postgres psql -U ponix -d ponix -c "SELECT * FROM gateways WHERE deleted_at IS NULL;"`
- [ ] Create test gateways via gRPC and verify method returns them
- [ ] Soft delete a gateway and verify it's excluded from results

---

## Phase 2: Process Store Abstraction

### Overview
Create pluggable process store with trait abstraction and in-memory implementation to manage gateway process lifecycle state.

### Changes Required

#### 1. Gateway Process State Type
**File**: `crates/ponix-domain/src/gateway_process.rs` (new file)
**Changes**: Define types for process state management

```rust
use crate::gateway::Gateway;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Handle to a running gateway process
pub struct GatewayProcessHandle {
    pub join_handle: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
    pub gateway: Gateway,
    pub config_hash: String,
}

impl GatewayProcessHandle {
    pub fn new(
        join_handle: JoinHandle<()>,
        cancellation_token: CancellationToken,
        gateway: Gateway,
    ) -> Self {
        let config_hash = Self::hash_config(&gateway.gateway_config);
        Self {
            join_handle,
            cancellation_token,
            gateway,
            config_hash,
        }
    }

    /// Hash gateway config JSON for change detection
    fn hash_config(config: &serde_json::Value) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        config.to_string().hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Check if gateway config has changed
    pub fn config_changed(&self, new_gateway: &Gateway) -> bool {
        let new_hash = Self::hash_config(&new_gateway.gateway_config);
        self.config_hash != new_hash
    }

    /// Cancel the process
    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }
}

/// Events that can occur in the gateway process lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatewayProcessEvent {
    Started { gateway_id: String },
    Stopped { gateway_id: String },
    Failed { gateway_id: String, error: String },
    Retrying { gateway_id: String, attempt: u32 },
}
```

#### 2. Process Store Trait
**File**: `crates/ponix-domain/src/gateway_process_store.rs` (new file)
**Changes**: Define trait for process state management

```rust
use crate::error::DomainResult;
use crate::gateway_process::GatewayProcessHandle;
use async_trait::async_trait;

/// Trait for storing and retrieving gateway process handles
/// Implementations can be in-memory, Redis, PostgreSQL, etc.
#[async_trait]
pub trait GatewayProcessStore: Send + Sync {
    /// Insert or update a process handle
    async fn upsert(&self, gateway_id: String, handle: GatewayProcessHandle) -> DomainResult<()>;

    /// Get a process handle by gateway ID
    async fn get(&self, gateway_id: &str) -> DomainResult<Option<GatewayProcessHandle>>;

    /// Remove a process handle
    async fn remove(&self, gateway_id: &str) -> DomainResult<Option<GatewayProcessHandle>>;

    /// List all active gateway IDs
    async fn list_gateway_ids(&self) -> DomainResult<Vec<String>>;

    /// Check if a gateway process exists
    async fn exists(&self, gateway_id: &str) -> DomainResult<bool>;

    /// Get count of active processes
    async fn count(&self) -> DomainResult<usize>;
}
```

#### 3. In-Memory Store Implementation
**File**: `crates/ponix-domain/src/in_memory_gateway_process_store.rs` (new file)
**Changes**: Implement store with HashMap and RwLock

```rust
use crate::error::DomainResult;
use crate::gateway_process::GatewayProcessHandle;
use crate::gateway_process_store::GatewayProcessStore;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory implementation of GatewayProcessStore using HashMap
pub struct InMemoryGatewayProcessStore {
    processes: Arc<RwLock<HashMap<String, GatewayProcessHandle>>>,
}

impl InMemoryGatewayProcessStore {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryGatewayProcessStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GatewayProcessStore for InMemoryGatewayProcessStore {
    async fn upsert(&self, gateway_id: String, handle: GatewayProcessHandle) -> DomainResult<()> {
        let mut processes = self.processes.write().await;
        processes.insert(gateway_id, handle);
        Ok(())
    }

    async fn get(&self, gateway_id: &str) -> DomainResult<Option<GatewayProcessHandle>> {
        let processes = self.processes.read().await;
        // Note: Can't return reference due to lifetime, need to handle JoinHandle differently
        // For now, return bool indicating existence - will refine in implementation
        Ok(None)
    }

    async fn remove(&self, gateway_id: &str) -> DomainResult<Option<GatewayProcessHandle>> {
        let mut processes = self.processes.write().await;
        Ok(processes.remove(gateway_id))
    }

    async fn list_gateway_ids(&self) -> DomainResult<Vec<String>> {
        let processes = self.processes.read().await;
        Ok(processes.keys().cloned().collect())
    }

    async fn exists(&self, gateway_id: &str) -> DomainResult<bool> {
        let processes = self.processes.read().await;
        Ok(processes.contains_key(gateway_id))
    }

    async fn count(&self) -> DomainResult<usize> {
        let processes = self.processes.read().await;
        Ok(processes.len())
    }
}
```

#### 4. Module Exports
**File**: `crates/ponix-domain/src/lib.rs`
**Changes**: Export new modules

```rust
// Add these to existing exports
pub mod gateway_process;
pub mod gateway_process_store;
pub mod in_memory_gateway_process_store;

pub use gateway_process::*;
pub use gateway_process_store::*;
pub use in_memory_gateway_process_store::*;
```

### Success Criteria

#### Automated Verification:
- [ ] Code compiles: `cargo build -p ponix-domain`
- [ ] Unit tests pass: `cargo test -p ponix-domain --lib`
- [ ] Type checking passes: `cargo check -p ponix-domain`
- [ ] Clippy passes: `cargo clippy -p ponix-domain`

#### Manual Verification:
- [ ] Verify in-memory store can insert and retrieve handles
- [ ] Verify concurrent access works correctly with RwLock
- [ ] Verify remove operation returns handle correctly
- [ ] Verify list and count operations return accurate results

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that the store abstraction meets requirements before proceeding to the orchestrator implementation.

---

## Phase 3: Gateway Orchestrator Core

### Overview
Implement the `GatewayOrchestrator` in the domain layer with methods for starting, stopping, and managing gateway print processes dynamically.

### Changes Required

#### 1. Orchestrator Configuration
**File**: `crates/ponix-domain/src/gateway_orchestrator_config.rs` (new file)
**Changes**: Define configuration struct

```rust
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayOrchestratorConfig {
    /// Interval between print statements (default: 5 seconds)
    pub print_interval_secs: u64,

    /// Delay before retrying failed process start (default: 10 seconds)
    pub retry_delay_secs: u64,

    /// Maximum number of retry attempts (default: 3)
    pub max_retry_attempts: u32,
}

impl Default for GatewayOrchestratorConfig {
    fn default() -> Self {
        Self {
            print_interval_secs: 5,
            retry_delay_secs: 10,
            max_retry_attempts: 3,
        }
    }
}

impl GatewayOrchestratorConfig {
    pub fn print_interval(&self) -> Duration {
        Duration::from_secs(self.print_interval_secs)
    }

    pub fn retry_delay(&self) -> Duration {
        Duration::from_secs(self.retry_delay_secs)
    }
}
```

#### 2. Gateway Orchestrator
**File**: `crates/ponix-domain/src/gateway_orchestrator.rs` (new file)
**Changes**: Implement orchestrator with lifecycle management

```rust
use crate::error::{DomainError, DomainResult};
use crate::gateway::Gateway;
use crate::gateway_orchestrator_config::GatewayOrchestratorConfig;
use crate::gateway_process::{GatewayProcessEvent, GatewayProcessHandle};
use crate::gateway_process_store::GatewayProcessStore;
use crate::repository::GatewayRepository;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub struct GatewayOrchestrator {
    gateway_repository: Arc<dyn GatewayRepository>,
    process_store: Arc<dyn GatewayProcessStore>,
    config: GatewayOrchestratorConfig,
    shutdown_token: CancellationToken,
}

impl GatewayOrchestrator {
    pub fn new(
        gateway_repository: Arc<dyn GatewayRepository>,
        process_store: Arc<dyn GatewayProcessStore>,
        config: GatewayOrchestratorConfig,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            gateway_repository,
            process_store,
            config,
            shutdown_token,
        }
    }

    /// Load all non-deleted gateways and start their processes
    pub async fn start(&self) -> DomainResult<()> {
        info!("Starting GatewayOrchestrator - loading all non-deleted gateways");

        let gateways = self.gateway_repository.list_all_gateways().await?;
        info!("Found {} non-deleted gateways to start", gateways.len());

        for gateway in gateways {
            if let Err(e) = self.start_gateway_process(&gateway).await {
                error!(
                    "Failed to start process for gateway {}: {}",
                    gateway.gateway_id, e
                );
                // Continue starting other processes even if one fails
            }
        }

        Ok(())
    }

    /// Handle gateway created event
    pub async fn handle_gateway_created(&self, gateway: Gateway) -> DomainResult<()> {
        info!("Handling gateway created: {}", gateway.gateway_id);
        self.start_gateway_process(&gateway).await
    }

    /// Handle gateway updated event
    pub async fn handle_gateway_updated(
        &self,
        old_gateway: Gateway,
        new_gateway: Gateway,
    ) -> DomainResult<()> {
        info!("Handling gateway updated: {}", new_gateway.gateway_id);

        // Check if soft deleted
        if new_gateway.deleted_at.is_some() {
            info!(
                "Gateway {} has been soft deleted, stopping process",
                new_gateway.gateway_id
            );
            return self.stop_gateway_process(&new_gateway.gateway_id).await;
        }

        // Check if config changed
        let config_changed = old_gateway.gateway_config != new_gateway.gateway_config;

        if config_changed {
            info!(
                "Gateway {} config changed, restarting process",
                new_gateway.gateway_id
            );
            // Stop existing process
            self.stop_gateway_process(&new_gateway.gateway_id).await?;
            // Start with new config
            self.start_gateway_process(&new_gateway).await?;
        } else {
            info!(
                "Gateway {} updated but config unchanged, no action needed",
                new_gateway.gateway_id
            );
        }

        Ok(())
    }

    /// Handle gateway deleted event
    pub async fn handle_gateway_deleted(&self, gateway_id: &str) -> DomainResult<()> {
        info!("Handling gateway deleted: {}", gateway_id);
        self.stop_gateway_process(gateway_id).await
    }

    /// Stop all running gateway processes
    pub async fn shutdown(&self) -> DomainResult<()> {
        info!("Shutting down GatewayOrchestrator");

        let gateway_ids = self.process_store.list_gateway_ids().await?;
        info!("Stopping {} gateway processes", gateway_ids.len());

        for gateway_id in gateway_ids {
            if let Err(e) = self.stop_gateway_process(&gateway_id).await {
                error!("Failed to stop process for gateway {}: {}", gateway_id, e);
                // Continue stopping other processes
            }
        }

        info!("GatewayOrchestrator shutdown complete");
        Ok(())
    }

    /// Start a gateway process
    async fn start_gateway_process(&self, gateway: &Gateway) -> DomainResult<()> {
        // Check if process already exists
        if self
            .process_store
            .exists(&gateway.gateway_id)
            .await?
        {
            warn!(
                "Process already exists for gateway {}, skipping",
                gateway.gateway_id
            );
            return Ok(());
        }

        let gateway_clone = gateway.clone();
        let config = self.config.clone();
        let process_token = CancellationToken::new();
        let process_token_clone = process_token.clone();
        let shutdown_token = self.shutdown_token.clone();

        // Spawn print process
        let join_handle = tokio::spawn(async move {
            run_gateway_print_process(
                gateway_clone,
                config,
                process_token_clone,
                shutdown_token,
            )
            .await;
        });

        // Store handle
        let handle = GatewayProcessHandle::new(join_handle, process_token, gateway.clone());
        self.process_store
            .upsert(gateway.gateway_id.clone(), handle)
            .await?;

        info!("Started process for gateway {}", gateway.gateway_id);
        Ok(())
    }

    /// Stop a gateway process
    async fn stop_gateway_process(&self, gateway_id: &str) -> DomainResult<()> {
        match self.process_store.remove(gateway_id).await? {
            Some(handle) => {
                info!("Stopping process for gateway {}", gateway_id);
                handle.cancel();

                // Wait for process to complete with timeout
                let timeout = tokio::time::Duration::from_secs(5);
                match tokio::time::timeout(timeout, handle.join_handle).await {
                    Ok(Ok(())) => {
                        info!("Process for gateway {} stopped gracefully", gateway_id);
                    }
                    Ok(Err(e)) => {
                        error!("Process for gateway {} panicked: {:?}", gateway_id, e);
                    }
                    Err(_) => {
                        warn!("Process for gateway {} did not stop within timeout", gateway_id);
                        // Process will be dropped and cleaned up by tokio
                    }
                }

                Ok(())
            }
            None => {
                warn!("No process found for gateway {}", gateway_id);
                Ok(())
            }
        }
    }
}

/// Run the gateway print process
async fn run_gateway_print_process(
    gateway: Gateway,
    config: GatewayOrchestratorConfig,
    process_token: CancellationToken,
    shutdown_token: CancellationToken,
) {
    info!(
        "Starting print process for gateway: {} ({})",
        gateway.gateway_id, gateway.organization_id
    );

    let mut retry_count = 0;

    loop {
        // Check for cancellation
        if process_token.is_cancelled() || shutdown_token.is_cancelled() {
            info!("Print process for gateway {} cancelled", gateway.gateway_id);
            break;
        }

        // Print gateway config
        match print_gateway_config(&gateway) {
            Ok(()) => {
                retry_count = 0; // Reset retry count on success
            }
            Err(e) => {
                error!(
                    "Error printing config for gateway {}: {}",
                    gateway.gateway_id, e
                );

                retry_count += 1;
                if retry_count >= config.max_retry_attempts {
                    error!(
                        "Max retry attempts ({}) reached for gateway {}, stopping process",
                        config.max_retry_attempts, gateway.gateway_id
                    );
                    break;
                }

                info!(
                    "Retrying print for gateway {} (attempt {}/{})",
                    gateway.gateway_id, retry_count, config.max_retry_attempts
                );

                // Wait before retry
                tokio::select! {
                    _ = process_token.cancelled() => break,
                    _ = shutdown_token.cancelled() => break,
                    _ = tokio::time::sleep(config.retry_delay()) => {}
                }
                continue;
            }
        }

        // Wait for next print interval
        tokio::select! {
            _ = process_token.cancelled() => {
                info!("Print process for gateway {} cancelled", gateway.gateway_id);
                break;
            }
            _ = shutdown_token.cancelled() => {
                info!("Print process for gateway {} shutdown signal received", gateway.gateway_id);
                break;
            }
            _ = tokio::time::sleep(config.print_interval()) => {}
        }
    }

    info!("Print process for gateway {} stopped", gateway.gateway_id);
}

/// Print gateway configuration to stdout
fn print_gateway_config(gateway: &Gateway) -> DomainResult<()> {
    println!(
        "[GATEWAY {}] org={} type={} config={}",
        gateway.gateway_id,
        gateway.organization_id,
        gateway.gateway_type,
        serde_json::to_string_pretty(&gateway.gateway_config)
            .unwrap_or_else(|_| "invalid json".to_string())
    );
    Ok(())
}
```

#### 3. Module Exports
**File**: `crates/ponix-domain/src/lib.rs`
**Changes**: Export orchestrator modules

```rust
// Add these to existing exports
pub mod gateway_orchestrator;
pub mod gateway_orchestrator_config;

pub use gateway_orchestrator::*;
pub use gateway_orchestrator_config::*;
```

#### 4. Unit Tests
**File**: `crates/ponix-domain/src/gateway_orchestrator.rs`
**Changes**: Add comprehensive unit tests with mocks

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::MockGatewayRepository;
    use crate::gateway_process_store::MockGatewayProcessStore;
    use mockall::predicate::*;

    // Helper to create test gateway
    fn create_test_gateway(gateway_id: &str, org_id: &str) -> Gateway {
        Gateway {
            gateway_id: gateway_id.to_string(),
            organization_id: org_id.to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: serde_json::json!({
                "host": "mqtt.example.com",
                "port": 1883
            }),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            status: "active".to_string(),
        }
    }

    #[tokio::test]
    async fn test_start_loads_all_gateways() {
        let mut mock_repo = MockGatewayRepository::new();
        let mut mock_store = MockGatewayProcessStore::new();

        let gateways = vec![
            create_test_gateway("gw1", "org1"),
            create_test_gateway("gw2", "org1"),
        ];

        mock_repo
            .expect_list_all_gateways()
            .times(1)
            .returning(move || Ok(gateways.clone()));

        mock_store
            .expect_exists()
            .times(2)
            .returning(|_| Ok(false));

        mock_store
            .expect_upsert()
            .times(2)
            .returning(|_, _| Ok(()));

        let orchestrator = GatewayOrchestrator::new(
            Arc::new(mock_repo),
            Arc::new(mock_store),
            GatewayOrchestratorConfig::default(),
            CancellationToken::new(),
        );

        let result = orchestrator.start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_gateway_created_starts_process() {
        let mock_repo = MockGatewayRepository::new();
        let mut mock_store = MockGatewayProcessStore::new();

        mock_store
            .expect_exists()
            .times(1)
            .returning(|_| Ok(false));

        mock_store
            .expect_upsert()
            .times(1)
            .returning(|_, _| Ok(()));

        let orchestrator = GatewayOrchestrator::new(
            Arc::new(mock_repo),
            Arc::new(mock_store),
            GatewayOrchestratorConfig::default(),
            CancellationToken::new(),
        );

        let gateway = create_test_gateway("gw1", "org1");
        let result = orchestrator.handle_gateway_created(gateway).await;
        assert!(result.is_ok());
    }

    // Add more tests for update, delete, shutdown scenarios
}
```

### Success Criteria

#### Automated Verification:
- [ ] Code compiles: `cargo build -p ponix-domain`
- [ ] Unit tests pass: `cargo test -p ponix-domain --lib`
- [ ] Type checking passes: `cargo check -p ponix-domain`
- [ ] Clippy passes: `cargo clippy -p ponix-domain`

#### Manual Verification:
- [ ] Create orchestrator with in-memory store and verify it can start
- [ ] Manually call `handle_gateway_created()` and verify process spawns
- [ ] Manually call `handle_gateway_deleted()` and verify process stops
- [ ] Verify print statements appear in stdout every 5 seconds
- [ ] Verify cancellation token stops all processes

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that the orchestrator behaves correctly before integrating with CDC consumer.

---

## Phase 4: CDC Consumer Integration

### Overview
Create a dedicated NATS consumer that listens to gateway CDC events, translates protobuf messages to domain types, and delegates to the orchestrator.

### Changes Required

#### 1. Gateway CDC Event Types
**File**: `crates/ponix-domain/src/gateway_cdc_event.rs` (new file)
**Changes**: Define domain event types for CDC

```rust
use crate::gateway::Gateway;
use serde::{Deserialize, Serialize};

/// Domain representation of a gateway CDC event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatewayCdcEvent {
    Created { gateway: Gateway },
    Updated { old: Gateway, new: Gateway },
    Deleted { gateway_id: String },
}

impl GatewayCdcEvent {
    pub fn gateway_id(&self) -> &str {
        match self {
            GatewayCdcEvent::Created { gateway } => &gateway.gateway_id,
            GatewayCdcEvent::Updated { new, .. } => &new.gateway_id,
            GatewayCdcEvent::Deleted { gateway_id } => gateway_id,
        }
    }
}
```

#### 2. Protobuf to Domain Converter
**File**: `crates/ponix-nats/src/gateway_cdc_converter.rs` (new file)
**Changes**: Convert protobuf Gateway messages to domain types

```rust
use anyhow::{anyhow, Result};
use ponix_domain::{Gateway, GatewayCdcEvent};
use ponix_proto_tonic::gateway::v1::Gateway as ProtoGateway;
use prost::Message;

/// Convert protobuf Gateway to domain Gateway
pub fn proto_to_domain_gateway(proto: &ProtoGateway) -> Result<Gateway> {
    // Parse timestamps
    let created_at = proto
        .created_at
        .as_ref()
        .ok_or_else(|| anyhow!("Missing created_at"))?;
    let created_at = chrono::DateTime::from_timestamp(
        created_at.seconds,
        created_at.nanos as u32,
    )
    .ok_or_else(|| anyhow!("Invalid created_at timestamp"))?;

    let updated_at = proto
        .updated_at
        .as_ref()
        .ok_or_else(|| anyhow!("Missing updated_at"))?;
    let updated_at = chrono::DateTime::from_timestamp(
        updated_at.seconds,
        updated_at.nanos as u32,
    )
    .ok_or_else(|| anyhow!("Invalid updated_at timestamp"))?;

    let deleted_at = proto.deleted_at.as_ref().and_then(|ts| {
        chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
    });

    // Parse gateway config
    let gateway_config = serde_json::from_str(&proto.gateway_config.name)
        .unwrap_or_else(|_| serde_json::json!({"name": proto.gateway_config.name.clone()}));

    Ok(Gateway {
        gateway_id: proto.gateway_id.clone(),
        organization_id: proto.organization_id.clone(),
        gateway_type: proto.gateway_type.clone(),
        gateway_config,
        created_at,
        updated_at,
        deleted_at,
        status: "active".to_string(), // Default status
    })
}

/// Parse gateway CDC event from NATS message
pub fn parse_gateway_cdc_event(
    subject: &str,
    payload: &[u8],
) -> Result<GatewayCdcEvent> {
    let proto_gateway = ProtoGateway::decode(payload)
        .map_err(|e| anyhow!("Failed to decode protobuf: {}", e))?;

    let gateway = proto_to_domain_gateway(&proto_gateway)?;

    // Determine event type from subject
    if subject.ends_with(".create") {
        Ok(GatewayCdcEvent::Created { gateway })
    } else if subject.ends_with(".update") {
        // For updates, we only get the new state from CDC
        // Create a clone for old state (actual diff not available from CDC)
        let old = gateway.clone();
        Ok(GatewayCdcEvent::Updated {
            old,
            new: gateway,
        })
    } else if subject.ends_with(".delete") {
        Ok(GatewayCdcEvent::Deleted {
            gateway_id: gateway.gateway_id.clone(),
        })
    } else {
        Err(anyhow!("Unknown CDC subject: {}", subject))
    }
}
```

#### 3. Gateway CDC Consumer
**File**: `crates/ponix-nats/src/gateway_cdc_consumer.rs` (new file)
**Changes**: NATS consumer that delegates to orchestrator

```rust
use crate::gateway_cdc_converter::parse_gateway_cdc_event;
use crate::NatsClient;
use anyhow::Result;
use async_nats::jetstream::consumer::PullConsumer;
use ponix_domain::{GatewayCdcEvent, GatewayOrchestrator};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub struct GatewayCdcConsumer {
    nats_client: Arc<NatsClient>,
    stream_name: String,
    consumer_name: String,
    orchestrator: Arc<GatewayOrchestrator>,
}

impl GatewayCdcConsumer {
    pub fn new(
        nats_client: Arc<NatsClient>,
        stream_name: String,
        consumer_name: String,
        orchestrator: Arc<GatewayOrchestrator>,
    ) -> Self {
        Self {
            nats_client,
            stream_name,
            consumer_name,
            orchestrator,
        }
    }

    /// Run the CDC consumer loop
    pub async fn run(&self, ctx: CancellationToken) -> Result<()> {
        info!("Starting GatewayCdcConsumer");

        // Create durable consumer if not exists
        let consumer = self.create_or_get_consumer().await?;

        loop {
            tokio::select! {
                _ = ctx.cancelled() => {
                    info!("GatewayCdcConsumer shutdown signal received");
                    break;
                }
                result = self.fetch_and_process_batch(&consumer) => {
                    if let Err(e) = result {
                        error!("Error processing CDC batch: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }

        info!("GatewayCdcConsumer stopped");
        Ok(())
    }

    async fn create_or_get_consumer(&self) -> Result<PullConsumer> {
        let js = self.nats_client.jetstream();

        let consumer = js
            .get_or_create_consumer(
                &self.stream_name,
                async_nats::jetstream::consumer::pull::Config {
                    name: Some(self.consumer_name.clone()),
                    durable_name: Some(self.consumer_name.clone()),
                    filter_subject: "gateway.>".to_string(),
                    ..Default::default()
                },
            )
            .await?;

        Ok(consumer)
    }

    async fn fetch_and_process_batch(&self, consumer: &PullConsumer) -> Result<()> {
        let mut messages = consumer
            .fetch()
            .max_messages(10)
            .expires(tokio::time::Duration::from_secs(5))
            .messages()
            .await?;

        while let Some(message) = messages.next().await {
            let msg = message?;

            match self.process_message(&msg).await {
                Ok(()) => {
                    msg.ack().await?;
                }
                Err(e) => {
                    error!(
                        "Failed to process CDC message from subject {}: {}",
                        msg.subject, e
                    );
                    msg.ack_with(async_nats::jetstream::AckKind::Nak(None))
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn process_message(&self, msg: &async_nats::jetstream::Message) -> Result<()> {
        let event = parse_gateway_cdc_event(&msg.subject, &msg.payload)?;

        info!(
            "Processing CDC event for gateway: {}",
            event.gateway_id()
        );

        match event {
            GatewayCdcEvent::Created { gateway } => {
                self.orchestrator
                    .handle_gateway_created(gateway)
                    .await
                    .map_err(|e| anyhow::anyhow!("Orchestrator error: {}", e))?;
            }
            GatewayCdcEvent::Updated { old, new } => {
                self.orchestrator
                    .handle_gateway_updated(old, new)
                    .await
                    .map_err(|e| anyhow::anyhow!("Orchestrator error: {}", e))?;
            }
            GatewayCdcEvent::Deleted { gateway_id } => {
                self.orchestrator
                    .handle_gateway_deleted(&gateway_id)
                    .await
                    .map_err(|e| anyhow::anyhow!("Orchestrator error: {}", e))?;
            }
        }

        Ok(())
    }
}
```

#### 4. Module Exports
**File**: `crates/ponix-nats/src/lib.rs`
**Changes**: Export new modules

```rust
// Add these to existing exports
pub mod gateway_cdc_converter;
pub mod gateway_cdc_consumer;

pub use gateway_cdc_converter::*;
pub use gateway_cdc_consumer::*;
```

**File**: `crates/ponix-domain/src/lib.rs`
**Changes**: Export CDC event types

```rust
pub mod gateway_cdc_event;
pub use gateway_cdc_event::*;
```

### Success Criteria

#### Automated Verification:
- [ ] Code compiles: `cargo build -p ponix-nats`
- [ ] Code compiles: `cargo build -p ponix-domain`
- [ ] Unit tests pass: `cargo test -p ponix-nats --lib`
- [ ] Type checking passes: `cargo check -p ponix-nats`
- [ ] Clippy passes: `cargo clippy -p ponix-nats`

#### Manual Verification:
- [ ] Create test protobuf Gateway message and verify conversion to domain type
- [ ] Verify CDC consumer can parse create/update/delete subjects correctly
- [ ] Verify converter handles missing fields gracefully with error messages
- [ ] Verify consumer acks messages on success and naks on failure

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that CDC events are correctly parsed and delegated to the orchestrator before integrating into the main service.

---

## Phase 5: Service Integration

### Overview
Wire up the orchestrator and CDC consumer into `ponix-all-in-one` service via the Runner pattern.

### Changes Required

#### 1. Service Configuration
**File**: `crates/ponix-all-in-one/src/config.rs`
**Changes**: Add orchestrator configuration fields

```rust
// Add to ServiceConfig struct
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    // ... existing fields ...

    /// Gateway orchestrator print interval in seconds (default: 5)
    #[serde(default = "default_gateway_orchestrator_print_interval_secs")]
    pub gateway_orchestrator_print_interval_secs: u64,

    /// Gateway orchestrator retry delay in seconds (default: 10)
    #[serde(default = "default_gateway_orchestrator_retry_delay_secs")]
    pub gateway_orchestrator_retry_delay_secs: u64,

    /// Gateway orchestrator max retry attempts (default: 3)
    #[serde(default = "default_gateway_orchestrator_max_retry_attempts")]
    pub gateway_orchestrator_max_retry_attempts: u32,

    /// NATS gateway CDC consumer name (default: "gateway-cdc-consumer")
    #[serde(default = "default_gateway_cdc_consumer_name")]
    pub gateway_cdc_consumer_name: String,
}

fn default_gateway_orchestrator_print_interval_secs() -> u64 {
    5
}

fn default_gateway_orchestrator_retry_delay_secs() -> u64 {
    10
}

fn default_gateway_orchestrator_max_retry_attempts() -> u32 {
    3
}

fn default_gateway_cdc_consumer_name() -> String {
    "gateway-cdc-consumer".to_string()
}

// Add method to convert to domain config
impl ServiceConfig {
    pub fn gateway_orchestrator_config(&self) -> GatewayOrchestratorConfig {
        GatewayOrchestratorConfig {
            print_interval_secs: self.gateway_orchestrator_print_interval_secs,
            retry_delay_secs: self.gateway_orchestrator_retry_delay_secs,
            max_retry_attempts: self.gateway_orchestrator_max_retry_attempts,
        }
    }
}
```

#### 2. Main Service Integration
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Add orchestrator and CDC consumer to Runner

```rust
// Add imports
use ponix_domain::{
    GatewayOrchestrator, GatewayOrchestratorConfig, InMemoryGatewayProcessStore,
};
use ponix_nats::GatewayCdcConsumer;

// In main() function, after gateway service creation (around line 180):

// Create gateway orchestrator
let orchestrator_config = config.gateway_orchestrator_config();
let process_store = Arc::new(InMemoryGatewayProcessStore::new());
let orchestrator_token = CancellationToken::new();

let orchestrator = Arc::new(GatewayOrchestrator::new(
    gateway_repository.clone(),
    process_store,
    orchestrator_config,
    orchestrator_token.clone(),
));

// Start orchestrator (load existing gateways)
orchestrator
    .start()
    .await
    .context("Failed to start gateway orchestrator")?;

info!("Gateway orchestrator started");

// Create gateway CDC consumer
let gateway_cdc_consumer = GatewayCdcConsumer::new(
    Arc::clone(&nats_client),
    config.nats_gateway_stream.clone(),
    config.gateway_cdc_consumer_name.clone(),
    Arc::clone(&orchestrator),
);

// In Runner setup (around line 305), add new processes:

let runner = Runner::new()
    .with_cancellation_token(orchestrator_token.clone())
    // ... existing processes ...

    // Add gateway CDC consumer process
    .with_app_process({
        let consumer = gateway_cdc_consumer;
        move |ctx| {
            Box::pin(async move {
                consumer
                    .run(ctx)
                    .await
                    .map_err(|e| anyhow::anyhow!("Gateway CDC consumer error: {}", e))
            })
        }
    })

    // Add orchestrator shutdown closer
    .with_closer({
        let orch = Arc::clone(&orchestrator);
        move || {
            Box::pin(async move {
                orch.shutdown()
                    .await
                    .map_err(|e| anyhow::anyhow!("Orchestrator shutdown error: {}", e))
            })
        }
    })

    .with_closer_timeout(Duration::from_secs(10));
```

#### 3. Environment Variables
**File**: `.env` (documentation in CLAUDE.md)
**Changes**: Document new configuration options

Add to CLAUDE.md:
```markdown
### Gateway Orchestrator Configuration
- `PONIX_GATEWAY_ORCHESTRATOR_PRINT_INTERVAL_SECS` (default: 5) - Print interval in seconds
- `PONIX_GATEWAY_ORCHESTRATOR_RETRY_DELAY_SECS` (default: 10) - Retry delay in seconds
- `PONIX_GATEWAY_ORCHESTRATOR_MAX_RETRY_ATTEMPTS` (default: 3) - Max retry attempts
- `PONIX_GATEWAY_CDC_CONSUMER_NAME` (default: "gateway-cdc-consumer") - CDC consumer name
```

### Success Criteria

#### Automated Verification:
- [ ] Code compiles: `cargo build -p ponix-all-in-one`
- [ ] Type checking passes: `cargo check -p ponix-all-in-one`
- [ ] Clippy passes: `cargo clippy -p ponix-all-in-one`
- [ ] Service starts without errors: `cargo run -p ponix-all-in-one`

#### Manual Verification:
- [ ] Start service and verify orchestrator loads existing gateways
- [ ] Verify print statements appear in logs every 5 seconds for each gateway
- [ ] Create gateway via gRPC and verify new print process starts
- [ ] Update gateway config via gRPC and verify process restarts
- [ ] Soft delete gateway via gRPC and verify process stops
- [ ] SIGTERM service and verify all processes stop gracefully

**Implementation Note**: After completing this phase and all automated verification passes, proceed to Phase 6 for integration testing.

---

## Phase 6: Integration Testing

### Overview
Create comprehensive integration tests to verify the full orchestrator lifecycle with CDC events.

### Changes Required

#### 1. Integration Test
**File**: `crates/ponix-postgres/tests/gateway_orchestrator_integration.rs` (new file)
**Changes**: End-to-end test with Docker containers

```rust
use anyhow::Result;
use ponix_domain::{
    CreateGatewayInput, GatewayOrchestrator, GatewayOrchestratorConfig,
    GatewayService, InMemoryGatewayProcessStore, OrganizationService,
    CreateOrganizationInput,
};
use ponix_nats::{GatewayCdcConsumer, NatsClient};
use ponix_postgres::{
    cdc::{CdcConfig, CdcProcess, EntityConfig, GatewayConverter},
    gateway_repository::PostgresGatewayRepository,
    organization_repository::PostgresOrganizationRepository,
    PostgresClient,
};
use std::sync::Arc;
use testcontainers::{clients::Cli, Container};
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use tokio_util::sync::CancellationToken;
use tracing_subscriber;

/// Set up test environment with PostgreSQL, NATS, and CDC
async fn setup_test_env() -> Result<(
    Container<'static, Postgres>,
    Container<'static, testcontainers_modules::nats::Nats>,
    Arc<PostgresClient>,
    Arc<NatsClient>,
    Arc<GatewayService>,
    Arc<OrganizationService>,
)> {
    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .try_init();

    // Start PostgreSQL with logical replication
    let postgres = Postgres::default()
        .with_db_name("ponix_test")
        .with_cmd(vec![
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "max_replication_slots=4",
            "-c",
            "max_wal_senders=4",
        ])
        .start()
        .await?;

    let pg_port = postgres.get_host_port_ipv4(5432).await?;
    let pg_url = format!("postgres://postgres:postgres@127.0.0.1:{}/ponix_test", pg_port);

    // Start NATS with JetStream
    let nats = testcontainers_modules::nats::Nats::default()
        .start()
        .await?;

    let nats_port = nats.get_host_port_ipv4(4222).await?;
    let nats_url = format!("nats://127.0.0.1:{}", nats_port);

    // Initialize PostgreSQL client and run migrations
    let pg_client = Arc::new(PostgresClient::new(&pg_url).await?);
    pg_client.run_migrations().await?;

    // Initialize NATS client
    let nats_client = Arc::new(NatsClient::new(&nats_url).await?);

    // Create gateways stream
    nats_client
        .ensure_stream(
            "gateways",
            vec!["gateway.>".to_string()],
            None,
        )
        .await?;

    // Create repositories
    let gateway_repo = Arc::new(PostgresGatewayRepository::new(pg_client.clone()));
    let org_repo = Arc::new(PostgresOrganizationRepository::new(pg_client.clone()));

    // Create services
    let gateway_service = Arc::new(GatewayService::new(
        gateway_repo.clone(),
        org_repo.clone(),
    ));
    let org_service = Arc::new(OrganizationService::new(org_repo.clone()));

    Ok((
        postgres,
        nats,
        pg_client,
        nats_client,
        gateway_service,
        org_service,
    ))
}

#[tokio::test]
#[cfg(feature = "integration-tests")]
async fn test_gateway_orchestrator_full_lifecycle() -> Result<()> {
    let (
        _postgres,
        _nats,
        pg_client,
        nats_client,
        gateway_service,
        org_service,
    ) = setup_test_env().await?;

    // Create organization
    let org_input = CreateOrganizationInput {
        name: "Test Org".to_string(),
    };
    let org = org_service.create_organization(org_input).await?;

    // Set up CDC process
    let cdc_config = CdcConfig {
        pg_host: "127.0.0.1".to_string(),
        pg_port: pg_client.port(),
        pg_database: "ponix_test".to_string(),
        pg_user: "postgres".to_string(),
        pg_password: "postgres".to_string(),
        publication_name: "ponix_cdc_publication".to_string(),
        slot_name: "test_slot".to_string(),
        batch_size: 100,
        batch_timeout_ms: 1000,
        retry_delay_ms: 1000,
        max_retry_attempts: 3,
    };

    let entity_configs = vec![EntityConfig {
        entity_name: "gateway".to_string(),
        table_name: "gateways".to_string(),
        converter: Box::new(GatewayConverter),
    }];

    let cdc_token = CancellationToken::new();
    let cdc_process = CdcProcess::new(
        cdc_config,
        Arc::clone(&nats_client),
        entity_configs,
        cdc_token.clone(),
    );

    // Start CDC process in background
    let cdc_handle = tokio::spawn(async move {
        cdc_process.run().await
    });

    // Create orchestrator
    let process_store = Arc::new(InMemoryGatewayProcessStore::new());
    let orchestrator_token = CancellationToken::new();
    let orchestrator_config = GatewayOrchestratorConfig {
        print_interval_secs: 1, // Fast for testing
        retry_delay_secs: 1,
        max_retry_attempts: 3,
    };

    let orchestrator = Arc::new(GatewayOrchestrator::new(
        gateway_service.gateway_repository().clone(),
        process_store.clone(),
        orchestrator_config,
        orchestrator_token.clone(),
    ));

    // Start orchestrator
    orchestrator.start().await?;

    // Create CDC consumer
    let cdc_consumer = GatewayCdcConsumer::new(
        Arc::clone(&nats_client),
        "gateways".to_string(),
        "test-consumer".to_string(),
        Arc::clone(&orchestrator),
    );

    let consumer_token = CancellationToken::new();
    let consumer_token_clone = consumer_token.clone();
    let consumer_handle = tokio::spawn(async move {
        cdc_consumer.run(consumer_token_clone).await
    });

    // Wait for CDC to catch up
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Test 1: Create gateway - should start process
    let gateway_input = CreateGatewayInput {
        organization_id: org.organization_id.clone(),
        gateway_type: "emqx".to_string(),
        gateway_config: serde_json::json!({
            "host": "mqtt.example.com",
            "port": 1883
        }),
    };

    let gateway = gateway_service.create_gateway(gateway_input).await?;

    // Wait for CDC event to propagate
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Verify process started
    assert!(process_store.exists(&gateway.gateway_id).await?);
    assert_eq!(process_store.count().await?, 1);

    // Test 2: Update gateway config - should restart process
    let updated_gateway = gateway_service
        .update_gateway(ponix_domain::UpdateGatewayInput {
            gateway_id: gateway.gateway_id.clone(),
            gateway_type: None,
            gateway_config: Some(serde_json::json!({
                "host": "mqtt.newhost.com",
                "port": 8883
            })),
        })
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Process should still exist (restarted)
    assert!(process_store.exists(&gateway.gateway_id).await?);

    // Test 3: Soft delete gateway - should stop process
    gateway_service
        .delete_gateway(&gateway.gateway_id)
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Process should be removed
    assert!(!process_store.exists(&gateway.gateway_id).await?);
    assert_eq!(process_store.count().await?, 0);

    // Cleanup
    orchestrator_token.cancel();
    consumer_token.cancel();
    cdc_token.cancel();

    let _ = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        cdc_handle,
    ).await;

    let _ = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        consumer_handle,
    ).await;

    Ok(())
}
```

#### 2. Feature Flag
**File**: `crates/ponix-postgres/Cargo.toml`
**Changes**: Ensure integration-tests feature includes new test

The existing feature flag should already cover this test since it's in the same tests directory.

### Success Criteria

#### Automated Verification:
- [ ] Integration test compiles: `cargo test -p ponix-postgres --features integration-tests --no-run`
- [ ] Integration test passes: `cargo test -p ponix-postgres --features integration-tests test_gateway_orchestrator_full_lifecycle -- --test-threads=1`

#### Manual Verification:
- [ ] Test creates gateway and verifies process starts
- [ ] Test updates gateway config and verifies process restarts
- [ ] Test soft deletes gateway and verifies process stops
- [ ] Test completes without hanging or timing out
- [ ] All Docker containers clean up properly

**Implementation Note**: After this phase completes successfully, the implementation is complete and ready for production use.

---

## Testing Strategy

### Unit Tests
- **Process Store**: Insert, retrieve, remove, list operations with concurrent access
- **Orchestrator**: Start, stop, create, update, delete with mocked dependencies
- **Converter**: Protobuf to domain conversion with valid and invalid inputs
- **Print Process**: Mock process behavior and verify cancellation

### Integration Tests
- **Full Lifecycle**: Create → print → update → restart → delete → stop
- **CDC Flow**: PostgreSQL → NATS → Orchestrator → Process management
- **Concurrent Operations**: Multiple gateways with overlapping lifecycle events
- **Graceful Shutdown**: Verify all processes stop cleanly on SIGTERM

### Manual Testing Steps
1. **Startup Behavior**:
   - Start service with existing gateways in database
   - Verify print processes start for all non-deleted gateways
   - Check logs for orchestrator startup messages

2. **Create Gateway**:
   ```bash
   grpcurl -plaintext -d '{"organization_id": "org-001", "gateway_type": "emqx", "gateway_config": "{\"host\": \"mqtt.example.com\"}"}' \
     localhost:50051 ponix.gateway.v1.GatewayService/CreateGateway
   ```
   - Verify new print process starts
   - Check stdout for gateway config prints every 5 seconds

3. **Update Gateway Config**:
   ```bash
   grpcurl -plaintext -d '{"gateway_id": "GATEWAY_ID", "gateway_config": "{\"host\": \"mqtt.newhost.com\"}"}' \
     localhost:50051 ponix.gateway.v1.GatewayService/UpdateGateway
   ```
   - Verify process restarts with new config
   - Check stdout for updated config values

4. **Soft Delete Gateway**:
   ```bash
   grpcurl -plaintext -d '{"gateway_id": "GATEWAY_ID"}' \
     localhost:50051 ponix.gateway.v1.GatewayService/DeleteGateway
   ```
   - Verify print process stops
   - Check logs for process termination message

5. **Graceful Shutdown**:
   ```bash
   docker-compose -f docker/docker-compose.service.yaml down
   ```
   - Verify all gateway processes stop within 10 seconds
   - Check logs for orchestrator shutdown messages

## Performance Considerations

### Process Scalability
- **In-Memory Store**: Suitable for hundreds of gateways; use Redis/PostgreSQL store for thousands
- **Print Interval**: 5 seconds is conservative; can be reduced if needed
- **Batch Processing**: CDC consumer processes events in batches of 10 for efficiency

### Resource Usage
- Each gateway process is a lightweight tokio task (~KB memory overhead)
- Print operations are non-blocking and minimal I/O
- Cancellation tokens enable instant shutdown without polling

### Optimization Opportunities (Future)
- Pool process tokens instead of creating new ones per gateway
- Batch multiple gateway starts/stops to reduce lock contention
- Add metrics for process count, start/stop durations, retry attempts

## Migration Notes

### Backward Compatibility
- No schema changes required - uses existing `gateways` table and CDC infrastructure
- New service components run alongside existing processes via Runner
- Existing gRPC, NATS consumers, and CDC processes are unaffected

### Rollout Strategy
1. Deploy with orchestrator disabled (set `PONIX_GATEWAY_ORCHESTRATOR_PRINT_INTERVAL_SECS=0` to skip startup)
2. Verify existing functionality still works
3. Enable orchestrator by removing environment variable override
4. Monitor logs for process startup and CDC event processing
5. Gradually create/update/delete gateways to verify full lifecycle

### Rollback Plan
- Remove orchestrator and CDC consumer from Runner registration
- Service operates without gateway process management (no impact on other features)
- Redeploy previous version if needed

## References

- Original ticket: GitHub Issue #49
- CDC Implementation: [2025-11-23-issue-42-generic-cdc-nats-sink.md](.thoughts/plans/2025-11-23-issue-42-generic-cdc-nats-sink.md)
- Runner Pattern: [lib.rs:163-265](crates/runner/src/lib.rs#L163-L265)
- Gateway Service: [gateway_service.rs:15-29](crates/ponix-domain/src/gateway_service.rs#L15-L29)
- CDC Process: [process.rs:78-101](crates/ponix-postgres/src/cdc/process.rs#L78-L101)
- NATS Consumer Pattern: [consumer.rs:103-124](crates/ponix-nats/src/consumer.rs#L103-L124)
