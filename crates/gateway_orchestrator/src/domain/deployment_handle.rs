use async_trait::async_trait;
use common::domain::{DomainResult, Gateway, GatewayConfig};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::time::Duration;

/// Trait representing a handle to a deployed gateway process.
///
/// Abstracts the deployment mechanism — the handle could represent an in-process
/// tokio task, a Docker container, a Kubernetes pod, etc.
#[async_trait]
pub trait DeploymentHandle: Send + Sync {
    /// The gateway this handle is managing
    fn gateway(&self) -> &Gateway;

    /// Hash of the gateway config at deployment time, for change detection
    fn config_hash(&self) -> &str;

    /// Cancel the running gateway process
    fn cancel(&self);

    /// Wait for the gateway process to stop, with a timeout
    async fn wait_for_stop(&mut self, timeout: Duration) -> anyhow::Result<()>;
}

/// Trait for storing and retrieving deployment handles.
///
/// Implementations can be in-memory, Redis, PostgreSQL, etc.
#[async_trait]
pub trait DeploymentHandleStore: Send + Sync {
    /// Insert or update a deployment handle
    async fn upsert(
        &self,
        gateway_id: String,
        handle: Box<dyn DeploymentHandle>,
    ) -> DomainResult<()>;

    /// Remove a deployment handle, returning it if it existed
    async fn remove(&self, gateway_id: &str) -> DomainResult<Option<Box<dyn DeploymentHandle>>>;

    /// List all active gateway IDs
    async fn list_gateway_ids(&self) -> DomainResult<Vec<String>>;

    /// Check if a deployment handle exists for the given gateway
    async fn exists(&self, gateway_id: &str) -> DomainResult<bool>;

    /// Get count of active deployments
    async fn count(&self) -> DomainResult<usize>;

    /// Get the config hash for a deployed gateway, if it exists
    async fn get_config_hash(&self, gateway_id: &str) -> DomainResult<Option<String>>;
}

/// The type of deployer to use for running gateway processes.
///
/// Only one deployer type is active at a time for the entire service.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployerType {
    /// Run gateway processes in-process via `tokio::spawn`
    #[default]
    InProcess,
}

impl fmt::Display for DeployerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InProcess => write!(f, "in_process"),
        }
    }
}

impl std::str::FromStr for DeployerType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "in_process" | "in-process" | "inprocess" => Ok(Self::InProcess),
            other => Err(format!(
                "unknown deployer type '{}', expected: in_process",
                other
            )),
        }
    }
}

/// Hash a gateway config for change detection.
///
/// Extracted as a free function so both deployers and orchestration logic
/// can use the same hashing strategy.
pub fn hash_gateway_config(config: &GatewayConfig) -> String {
    let mut hasher = DefaultHasher::new();
    match config {
        GatewayConfig::Emqx(emqx) => {
            emqx.broker_url.hash(&mut hasher);
        }
    }
    format!("{:x}", hasher.finish())
}
