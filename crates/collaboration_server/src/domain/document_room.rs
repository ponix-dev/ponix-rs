use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{mpsc, RwLock};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

pub type ClientId = u64;

pub struct ConnectedClient {
    pub user_id: String,
    pub sender: mpsc::Sender<Vec<u8>>,
}

pub struct DocumentRoom {
    document_id: String,
    doc: RwLock<Doc>,
    clients: RwLock<HashMap<ClientId, ConnectedClient>>,
    next_client_id: AtomicU64,
}

impl DocumentRoom {
    /// Create a new room, initializing the Yrs Doc from stored state
    pub fn new(document_id: String, yrs_state: &[u8]) -> Result<Self, anyhow::Error> {
        let doc = Doc::new();
        if !yrs_state.is_empty() {
            let mut txn = doc.transact_mut();
            let update = Update::decode_v1(yrs_state)
                .map_err(|e| anyhow::anyhow!("failed to decode yrs state: {}", e))?;
            txn.apply_update(update)
                .map_err(|e| anyhow::anyhow!("failed to apply yrs state: {}", e))?;
        }

        Ok(Self {
            document_id,
            doc: RwLock::new(doc),
            clients: RwLock::new(HashMap::new()),
            next_client_id: AtomicU64::new(1),
        })
    }

    /// Apply replay updates (from JetStream catchup)
    pub async fn apply_update(&self, update: &[u8]) -> Result<(), anyhow::Error> {
        let doc = self.doc.write().await;
        let mut txn = doc.transact_mut();
        let decoded = Update::decode_v1(update)
            .map_err(|e| anyhow::anyhow!("failed to decode update: {}", e))?;
        txn.apply_update(decoded)
            .map_err(|e| anyhow::anyhow!("failed to apply update: {}", e))?;
        Ok(())
    }

    /// Add a client. Returns (client_id, state_vector, full_update)
    pub async fn add_client(
        &self,
        user_id: String,
        sender: mpsc::Sender<Vec<u8>>,
    ) -> (ClientId, Vec<u8>, Vec<u8>) {
        let client_id = self.next_client_id.fetch_add(1, Ordering::Relaxed);

        let (state_vector, full_update) = {
            let doc = self.doc.read().await;
            let txn = doc.transact();
            let sv = txn.state_vector().encode_v1();
            let update = txn.encode_state_as_update_v1(&StateVector::default());
            (sv, update)
        };

        self.clients
            .write()
            .await
            .insert(client_id, ConnectedClient { user_id, sender });

        (client_id, state_vector, full_update)
    }

    /// Remove a client. Returns remaining client count
    pub async fn remove_client(&self, client_id: ClientId) -> usize {
        let mut clients = self.clients.write().await;
        clients.remove(&client_id);
        clients.len()
    }

    /// Handle an update from a local client.
    /// Applies to doc, broadcasts to all other local clients.
    /// Returns the raw update bytes for NATS publishing.
    pub async fn handle_client_update(
        &self,
        from_client: ClientId,
        update: &[u8],
    ) -> Result<Vec<u8>, anyhow::Error> {
        // Apply to the doc
        {
            let doc = self.doc.write().await;
            let mut txn = doc.transact_mut();
            let decoded = Update::decode_v1(update)
                .map_err(|e| anyhow::anyhow!("failed to decode client update: {}", e))?;
            txn.apply_update(decoded)
                .map_err(|e| anyhow::anyhow!("failed to apply client update: {}", e))?;
        }

        // Broadcast to other local clients (wrap as sync update message)
        let msg = crate::domain::encode_update(update);
        let clients = self.clients.read().await;
        for (&id, client) in clients.iter() {
            if id != from_client {
                let _ = client.sender.try_send(msg.clone());
            }
        }

        Ok(update.to_vec())
    }

    /// Handle an update from NATS (another instance).
    /// Applies to doc, broadcasts to ALL local clients.
    pub async fn handle_remote_update(&self, update: &[u8]) -> Result<(), anyhow::Error> {
        // Apply to the doc
        {
            let doc = self.doc.write().await;
            let mut txn = doc.transact_mut();
            let decoded = Update::decode_v1(update)
                .map_err(|e| anyhow::anyhow!("failed to decode remote update: {}", e))?;
            txn.apply_update(decoded)
                .map_err(|e| anyhow::anyhow!("failed to apply remote update: {}", e))?;
        }

        // Broadcast to all local clients
        let msg = crate::domain::encode_update(update);
        let clients = self.clients.read().await;
        for client in clients.values() {
            let _ = client.sender.try_send(msg.clone());
        }

        Ok(())
    }

    /// Compute diff for a client's SyncStep1 (their state vector).
    /// Returns the update bytes the client is missing.
    pub async fn compute_diff(&self, client_state_vector: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
        let sv = StateVector::decode_v1(client_state_vector)
            .map_err(|e| anyhow::anyhow!("failed to decode client state vector: {}", e))?;

        let doc = self.doc.read().await;
        let txn = doc.transact();
        let diff = txn.encode_state_as_update_v1(&sv);
        Ok(diff)
    }

    /// Send data to a specific client
    pub async fn send_to_client(&self, client_id: ClientId, data: Vec<u8>) {
        let clients = self.clients.read().await;
        if let Some(client) = clients.get(&client_id) {
            let _ = client.sender.try_send(data);
        }
    }

    /// Current client count
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Get read access to the clients map (for awareness broadcasting)
    pub async fn clients_read(
        &self,
    ) -> tokio::sync::RwLockReadGuard<'_, HashMap<ClientId, ConnectedClient>> {
        self.clients.read().await
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::yrs::create_empty_document;
    use yrs::{GetString, Text};

    fn make_update(text: &str) -> Vec<u8> {
        let doc = Doc::new();
        let (state, _) = create_empty_document();
        {
            let mut txn = doc.transact_mut();
            let update = Update::decode_v1(&state).unwrap();
            txn.apply_update(update).unwrap();
        }
        let t = doc.get_or_insert_text("content");
        let mut txn = doc.transact_mut();
        t.insert(&mut txn, 0, text);
        txn.encode_update_v1()
    }

    #[tokio::test]
    async fn test_create_room_with_empty_state() {
        let (state, _) = create_empty_document();
        let room = DocumentRoom::new("doc-1".to_string(), &state).unwrap();
        assert_eq!(room.document_id(), "doc-1");
        assert_eq!(room.client_count().await, 0);
    }

    #[tokio::test]
    async fn test_add_remove_clients() {
        let (state, _) = create_empty_document();
        let room = DocumentRoom::new("doc-1".to_string(), &state).unwrap();

        let (tx1, _rx1) = mpsc::channel(16);
        let (tx2, _rx2) = mpsc::channel(16);

        let (id1, _sv, _update) = room.add_client("user-1".to_string(), tx1).await;
        assert_eq!(room.client_count().await, 1);

        let (id2, _, _) = room.add_client("user-2".to_string(), tx2).await;
        assert_eq!(room.client_count().await, 2);

        let remaining = room.remove_client(id1).await;
        assert_eq!(remaining, 1);

        let remaining = room.remove_client(id2).await;
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn test_client_update_broadcasts_to_others() {
        let (state, _) = create_empty_document();
        let room = DocumentRoom::new("doc-1".to_string(), &state).unwrap();

        let (tx1, _rx1) = mpsc::channel(16);
        let (tx2, mut rx2) = mpsc::channel(16);

        let (id1, _, _) = room.add_client("user-1".to_string(), tx1).await;
        let (_id2, _, _) = room.add_client("user-2".to_string(), tx2).await;

        let update = make_update("hello");
        room.handle_client_update(id1, &update).await.unwrap();

        // Client 2 should receive the update
        let msg = rx2.try_recv().unwrap();
        assert!(!msg.is_empty());
    }

    #[tokio::test]
    async fn test_client_update_not_echoed_to_sender() {
        let (state, _) = create_empty_document();
        let room = DocumentRoom::new("doc-1".to_string(), &state).unwrap();

        let (tx1, mut rx1) = mpsc::channel(16);
        let (id1, _, _) = room.add_client("user-1".to_string(), tx1).await;

        let update = make_update("hello");
        room.handle_client_update(id1, &update).await.unwrap();

        // Client 1 should NOT receive the update back
        assert!(rx1.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_remote_update_broadcasts_to_all() {
        let (state, _) = create_empty_document();
        let room = DocumentRoom::new("doc-1".to_string(), &state).unwrap();

        let (tx1, mut rx1) = mpsc::channel(16);
        let (tx2, mut rx2) = mpsc::channel(16);

        room.add_client("user-1".to_string(), tx1).await;
        room.add_client("user-2".to_string(), tx2).await;

        let update = make_update("remote");
        room.handle_remote_update(&update).await.unwrap();

        // Both clients should receive the update
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_compute_diff() {
        let (state, sv) = create_empty_document();
        let room = DocumentRoom::new("doc-1".to_string(), &state).unwrap();

        // Apply an update to the room's doc
        let update = make_update("test content");
        room.apply_update(&update).await.unwrap();

        // The diff from the original state vector should contain the new content
        let diff = room.compute_diff(&sv).await.unwrap();
        assert!(!diff.is_empty());

        // Apply the diff to a fresh doc and verify content
        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();
            let u = Update::decode_v1(&state).unwrap();
            txn.apply_update(u).unwrap();
        }
        {
            let mut txn = doc.transact_mut();
            let u = Update::decode_v1(&diff).unwrap();
            txn.apply_update(u).unwrap();
        }
        let t = doc.get_or_insert_text("content");
        let txn = doc.transact();
        assert_eq!(t.get_string(&txn), "test content");
    }

    #[tokio::test]
    async fn test_initial_sync_data() {
        let (state, _) = create_empty_document();
        let room = DocumentRoom::new("doc-1".to_string(), &state).unwrap();

        let (tx, _rx) = mpsc::channel(16);
        let (_id, sv, full_update) = room.add_client("user-1".to_string(), tx).await;

        // State vector and update should be non-empty
        assert!(!sv.is_empty());
        assert!(!full_update.is_empty());

        // The full update should be loadable by a fresh doc
        let doc = Doc::new();
        let mut txn = doc.transact_mut();
        let u = Update::decode_v1(&full_update).unwrap();
        txn.apply_update(u).unwrap();
    }
}
