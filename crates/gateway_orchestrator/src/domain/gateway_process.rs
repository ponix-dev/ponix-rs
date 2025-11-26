use async_trait::async_trait;
use common::domain::{DomainResult, Gateway, GatewayConfig};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
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

    /// Hash gateway config for change detection
    fn hash_config(config: &GatewayConfig) -> String {
        let mut hasher = DefaultHasher::new();
        // Hash the config fields to detect changes
        match config {
            GatewayConfig::Emqx(emqx) => {
                emqx.broker_url.hash(&mut hasher);
            }
        }
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
