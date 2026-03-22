use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use common::domain::DocumentRepository;

use crate::domain::encode_awareness_message;
use crate::domain::DocumentAwarenessManager;
use crate::domain::DocumentRelay;
use crate::domain::DocumentRoom;

pub struct RoomManager {
    rooms: RwLock<HashMap<String, Arc<DocumentRoom>>>,
    awareness_managers: RwLock<HashMap<String, Arc<DocumentAwarenessManager>>>,
    creating_rooms: Mutex<HashSet<String>>,
    document_repository: Arc<dyn DocumentRepository>,
    relay: Arc<dyn DocumentRelay>,
    document_updates_stream: String,
}

impl RoomManager {
    pub fn new(
        document_repository: Arc<dyn DocumentRepository>,
        relay: Arc<dyn DocumentRelay>,
        document_updates_stream: String,
    ) -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            awareness_managers: RwLock::new(HashMap::new()),
            creating_rooms: Mutex::new(HashSet::new()),
            document_repository,
            relay,
            document_updates_stream,
        }
    }

    /// Get or create a room for a document.
    /// If creating: loads yrs_state from PG, replays JetStream, starts NATS subscription.
    /// Returns None if document doesn't exist.
    ///
    /// Uses per-document creation locking to prevent duplicate I/O and leaked NATS subscriptions
    /// when multiple tasks request the same document concurrently.
    pub async fn get_or_create_room(
        &self,
        document_id: &str,
    ) -> Result<Option<Arc<DocumentRoom>>, anyhow::Error> {
        loop {
            // Fast path: room already exists
            {
                let rooms = self.rooms.read().await;
                if let Some(room) = rooms.get(document_id) {
                    return Ok(Some(room.clone()));
                }
            }

            // Acquire per-document creation lock
            {
                let mut creating = self.creating_rooms.lock().await;
                if creating.contains(document_id) {
                    // Another task is creating this room — yield and retry
                    drop(creating);
                    tokio::task::yield_now().await;
                    continue;
                }
                creating.insert(document_id.to_string());
            }

            // We hold the creation lock — ensure cleanup on all paths
            let result = self.create_room(document_id).await;

            // Remove from creating set
            self.creating_rooms.lock().await.remove(document_id);

            return result;
        }
    }

    async fn create_room(
        &self,
        document_id: &str,
    ) -> Result<Option<Arc<DocumentRoom>>, anyhow::Error> {
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

        // Build sync callback: applies remote updates to room and broadcasts to local clients
        let sync_room = room.clone();
        let sync_handler: crate::domain::RemoteUpdateHandler = Box::new(move |update: Vec<u8>| {
            let room = sync_room.clone();
            Box::pin(async move {
                if let Err(e) = room.handle_remote_update(&update).await {
                    tracing::warn!(error = %e, "failed to apply remote update");
                }
            })
        });

        // Start JetStream subscription for cross-instance relay (replays from updated_at then continues)
        self.relay
            .subscribe_updates(
                document_id,
                document.updated_at,
                &self.document_updates_stream,
                sync_handler,
            )
            .await?;

        // Create awareness manager and start NATS awareness subscription (one per room)
        let awareness_manager = Arc::new(DocumentAwarenessManager::new(document_id.to_string()));
        self.awareness_managers
            .write()
            .await
            .insert(document_id.to_string(), awareness_manager.clone());

        // Build awareness callback: store remote awareness and relay to local clients
        let awareness_room = room.clone();
        let awareness_mgr = awareness_manager.clone();
        let awareness_handler: crate::domain::RemoteUpdateHandler =
            Box::new(move |data: Vec<u8>| {
                let room = awareness_room.clone();
                let mgr = awareness_mgr.clone();
                Box::pin(async move {
                    // Store remote awareness so new clients get full state on connect
                    mgr.store_remote_update(&data).await;
                    // Relay raw awareness bytes — prepend 0x01 tag for WebSocket clients
                    let msg = encode_awareness_message(&data);
                    room.broadcast_to_all(msg).await;
                })
            });

        self.relay
            .subscribe_awareness(document_id, awareness_handler)
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

    /// Get the relay (for publish/unsubscribe from connection handler)
    pub fn relay(&self) -> &Arc<dyn DocumentRelay> {
        &self.relay
    }

    /// Get the per-document awareness manager (created alongside the room in `create_room`).
    pub async fn get_awareness(&self, document_id: &str) -> Option<Arc<DocumentAwarenessManager>> {
        self.awareness_managers
            .read()
            .await
            .get(document_id)
            .cloned()
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

    use crate::nats::NatsDocumentRelay;

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

    fn make_relay_stub() -> Arc<dyn DocumentRelay> {
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
            make_relay_stub(),
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
            make_relay_stub(),
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
            make_relay_stub(),
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
            make_relay_stub(),
            "document_sync".to_string(),
        );

        manager.get_or_create_room("doc-1").await.unwrap();
        assert_eq!(manager.active_room_count().await, 1);

        manager.remove_room("doc-1").await;
        assert_eq!(manager.active_room_count().await, 0);
    }
}
