use crate::domain::{GatewayProcessHandle, GatewayProcessStore};
use async_trait::async_trait;
use common::domain::DomainResult;
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
        // Note: This removes the handle from the store because JoinHandle
        // can't be cloned. Use exists() to check without removing.
        let mut processes = self.processes.write().await;
        Ok(processes.remove(gateway_id))
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
