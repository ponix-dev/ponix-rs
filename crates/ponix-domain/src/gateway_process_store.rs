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
