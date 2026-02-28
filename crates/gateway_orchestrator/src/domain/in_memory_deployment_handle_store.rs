use crate::domain::{DeploymentHandle, DeploymentHandleStore};
use async_trait::async_trait;
use common::domain::DomainResult;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// In-memory implementation of `DeploymentHandleStore` using a `HashMap`.
pub struct InMemoryDeploymentHandleStore {
    handles: RwLock<HashMap<String, Box<dyn DeploymentHandle>>>,
}

impl InMemoryDeploymentHandleStore {
    pub fn new() -> Self {
        Self {
            handles: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryDeploymentHandleStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DeploymentHandleStore for InMemoryDeploymentHandleStore {
    async fn upsert(
        &self,
        gateway_id: String,
        handle: Box<dyn DeploymentHandle>,
    ) -> DomainResult<()> {
        let mut handles = self.handles.write().await;
        handles.insert(gateway_id, handle);
        Ok(())
    }

    async fn remove(&self, gateway_id: &str) -> DomainResult<Option<Box<dyn DeploymentHandle>>> {
        let mut handles = self.handles.write().await;
        Ok(handles.remove(gateway_id))
    }

    async fn list_gateway_ids(&self) -> DomainResult<Vec<String>> {
        let handles = self.handles.read().await;
        Ok(handles.keys().cloned().collect())
    }

    async fn exists(&self, gateway_id: &str) -> DomainResult<bool> {
        let handles = self.handles.read().await;
        Ok(handles.contains_key(gateway_id))
    }

    async fn count(&self) -> DomainResult<usize> {
        let handles = self.handles.read().await;
        Ok(handles.len())
    }

    async fn get_config_hash(&self, gateway_id: &str) -> DomainResult<Option<String>> {
        let handles = self.handles.read().await;
        Ok(handles
            .get(gateway_id)
            .map(|handle| handle.config_hash().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DeploymentHandle;
    use common::domain::{EmqxGatewayConfig, Gateway, GatewayConfig};
    use std::time::Duration;

    struct StubHandle {
        gateway: Gateway,
    }

    #[async_trait]
    impl DeploymentHandle for StubHandle {
        fn gateway(&self) -> &Gateway {
            &self.gateway
        }
        fn config_hash(&self) -> &str {
            "stub-hash"
        }
        fn cancel(&self) {}
        async fn wait_for_stop(&mut self, _timeout: Duration) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn stub_gateway(id: &str) -> Gateway {
        Gateway {
            gateway_id: id.to_string(),
            organization_id: "org-001".to_string(),
            name: format!("Gateway {}", id),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: "mqtt://localhost:1883".to_string(),
            }),
            created_at: Some(chrono::Utc::now()),
            updated_at: None,
            deleted_at: None,
        }
    }

    fn stub_handle(id: &str) -> Box<dyn DeploymentHandle> {
        Box::new(StubHandle {
            gateway: stub_gateway(id),
        })
    }

    #[tokio::test]
    async fn test_upsert_and_exists() {
        let store = InMemoryDeploymentHandleStore::new();
        store
            .upsert("gw1".to_string(), stub_handle("gw1"))
            .await
            .unwrap();
        assert!(store.exists("gw1").await.unwrap());
        assert!(!store.exists("gw2").await.unwrap());
    }

    #[tokio::test]
    async fn test_remove() {
        let store = InMemoryDeploymentHandleStore::new();
        store
            .upsert("gw1".to_string(), stub_handle("gw1"))
            .await
            .unwrap();

        let removed = store.remove("gw1").await.unwrap();
        assert!(removed.is_some());
        assert!(!store.exists("gw1").await.unwrap());

        let removed_again = store.remove("gw1").await.unwrap();
        assert!(removed_again.is_none());
    }

    #[tokio::test]
    async fn test_list_gateway_ids() {
        let store = InMemoryDeploymentHandleStore::new();
        store
            .upsert("gw1".to_string(), stub_handle("gw1"))
            .await
            .unwrap();
        store
            .upsert("gw2".to_string(), stub_handle("gw2"))
            .await
            .unwrap();

        let mut ids = store.list_gateway_ids().await.unwrap();
        ids.sort();
        assert_eq!(ids, vec!["gw1", "gw2"]);
    }

    #[tokio::test]
    async fn test_count() {
        let store = InMemoryDeploymentHandleStore::new();
        assert_eq!(store.count().await.unwrap(), 0);

        store
            .upsert("gw1".to_string(), stub_handle("gw1"))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        store
            .upsert("gw2".to_string(), stub_handle("gw2"))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 2);

        store.remove("gw1").await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);
    }
}
