use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use common::domain::DocumentRepository;

use crate::domain::DocumentAwarenessManager;
use crate::domain::DocumentRoom;
use crate::nats::NatsDocumentRelay;

pub struct RoomManager {
    rooms: RwLock<HashMap<String, Arc<DocumentRoom>>>,
    awareness_managers: RwLock<HashMap<String, Arc<DocumentAwarenessManager>>>,
    document_repository: Arc<dyn DocumentRepository>,
    nats_relay: Arc<NatsDocumentRelay>,
    document_updates_stream: String,
}

impl RoomManager {
    pub fn new(
        document_repository: Arc<dyn DocumentRepository>,
        nats_relay: Arc<NatsDocumentRelay>,
        document_updates_stream: String,
    ) -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            awareness_managers: RwLock::new(HashMap::new()),
            document_repository,
            nats_relay,
            document_updates_stream,
        }
    }

    /// Get or create a room for a document.
    /// If creating: loads yrs_state from PG, replays JetStream, starts NATS subscription.
    /// Returns None if document doesn't exist.
    pub async fn get_or_create_room(
        &self,
        document_id: &str,
    ) -> Result<Option<Arc<DocumentRoom>>, anyhow::Error> {
        // Fast path: room already exists
        {
            let rooms = self.rooms.read().await;
            if let Some(room) = rooms.get(document_id) {
                return Ok(Some(room.clone()));
            }
        }

        // Slow path: load document from PG and create room
        let document = self
            .document_repository
            .get_document_by_id(document_id)
            .await
            .map_err(|e| anyhow::anyhow!("failed to get document: {}", e))?;

        let document = match document {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let room = Arc::new(DocumentRoom::new(
            document_id.to_string(),
            &document.yrs_state,
        )?);

        // Start JetStream subscription for cross-instance relay (replays from updated_at then continues)
        self.nats_relay
            .subscribe(
                document_id,
                room.clone(),
                document.updated_at,
                &self.document_updates_stream,
            )
            .await?;

        // Insert into map
        self.rooms
            .write()
            .await
            .insert(document_id.to_string(), room.clone());

        Ok(Some(room))
    }

    /// Remove a room (called when last client disconnects)
    pub async fn remove_room(&self, document_id: &str) {
        self.rooms.write().await.remove(document_id);
    }

    /// Get or create a per-document awareness manager
    pub async fn get_or_create_awareness(
        &self,
        document_id: &str,
    ) -> Arc<DocumentAwarenessManager> {
        {
            let managers = self.awareness_managers.read().await;
            if let Some(manager) = managers.get(document_id) {
                return manager.clone();
            }
        }

        let mut managers = self.awareness_managers.write().await;
        // Double-check after acquiring write lock
        if let Some(manager) = managers.get(document_id) {
            return manager.clone();
        }

        let manager = Arc::new(DocumentAwarenessManager::new(document_id.to_string()));
        managers.insert(document_id.to_string(), manager.clone());
        manager
    }

    /// Remove awareness manager (called when last client disconnects)
    pub async fn remove_awareness(&self, document_id: &str) {
        self.awareness_managers.write().await.remove(document_id);
    }

    /// Check active room count (for health checks)
    pub async fn active_room_count(&self) -> usize {
        self.rooms.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::{Document, MockDocumentRepository};
    use common::yrs::create_empty_document;

    fn make_test_document(document_id: &str) -> Document {
        let (yrs_state, yrs_state_vector) = create_empty_document();
        Document {
            document_id: document_id.to_string(),
            organization_id: "org-1".to_string(),
            name: "Test Doc".to_string(),
            yrs_state,
            yrs_state_vector,
            content_text: String::new(),
            content_html: String::new(),
            metadata: serde_json::json!({}),
            deleted_at: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        }
    }

    fn make_nats_relay_stub() -> Arc<NatsDocumentRelay> {
        Arc::new(NatsDocumentRelay::new_stub())
    }

    #[tokio::test]
    async fn test_get_or_create_room_nonexistent_document() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .returning(|_| Ok(None));

        let manager = RoomManager::new(
            Arc::new(mock_repo),
            make_nats_relay_stub(),
            "document_sync".to_string(),
        );

        let result = manager.get_or_create_room("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_or_create_room_existing_document() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .returning(|id| Ok(Some(make_test_document(id))));

        let manager = RoomManager::new(
            Arc::new(mock_repo),
            make_nats_relay_stub(),
            "document_sync".to_string(),
        );

        let room = manager.get_or_create_room("doc-1").await.unwrap();
        assert!(room.is_some());
        assert_eq!(room.unwrap().document_id(), "doc-1");
    }

    #[tokio::test]
    async fn test_returns_existing_room_on_second_call() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .times(1) // Should only be called once
            .returning(|id| Ok(Some(make_test_document(id))));

        let manager = RoomManager::new(
            Arc::new(mock_repo),
            make_nats_relay_stub(),
            "document_sync".to_string(),
        );

        let room1 = manager.get_or_create_room("doc-1").await.unwrap().unwrap();
        let room2 = manager.get_or_create_room("doc-1").await.unwrap().unwrap();
        assert_eq!(room1.document_id(), room2.document_id());
    }

    #[tokio::test]
    async fn test_remove_room() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .returning(|id| Ok(Some(make_test_document(id))));

        let manager = RoomManager::new(
            Arc::new(mock_repo),
            make_nats_relay_stub(),
            "document_sync".to_string(),
        );

        manager.get_or_create_room("doc-1").await.unwrap();
        assert_eq!(manager.active_room_count().await, 1);

        manager.remove_room("doc-1").await;
        assert_eq!(manager.active_room_count().await, 0);
    }
}
