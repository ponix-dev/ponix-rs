use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::RwLock;

use crate::domain::awareness::{CursorPosition, UserPresence};
use crate::domain::connected_user::ConnectedUser;

pub struct AwarenessClientState {
    pub clock: u32,
    pub presence: UserPresence,
}

pub struct DocumentAwarenessManager {
    document_id: String,
    clients: RwLock<HashMap<u64, AwarenessClientState>>,
    next_client_id: AtomicU64,
}

impl DocumentAwarenessManager {
    pub fn new(document_id: String) -> Self {
        Self {
            document_id,
            clients: RwLock::new(HashMap::new()),
            next_client_id: AtomicU64::new(1),
        }
    }

    /// Add a client with server-authoritative identity. Returns the assigned client_id
    /// and the encoded awareness update to broadcast.
    pub async fn add_client(&self, user: &ConnectedUser) -> (u64, Vec<u8>) {
        let client_id = self.next_client_id.fetch_add(1, Ordering::Relaxed);
        let presence = user.to_presence();
        let clock = 1;

        let update = encode_awareness_update(client_id, clock, &presence);

        self.clients
            .write()
            .await
            .insert(client_id, AwarenessClientState { clock, presence });

        (client_id, update)
    }

    /// Remove a client. Returns the encoded removal update for broadcasting.
    pub async fn remove_client(&self, client_id: u64) -> Option<Vec<u8>> {
        let mut clients = self.clients.write().await;
        if clients.remove(&client_id).is_some() {
            Some(encode_awareness_removal(client_id))
        } else {
            None
        }
    }

    /// Update a client's cursor position, keeping server-authoritative identity.
    /// Returns the encoded awareness update for broadcasting.
    pub async fn apply_client_cursor_update(
        &self,
        client_id: u64,
        cursor: Option<CursorPosition>,
    ) -> Option<Vec<u8>> {
        let mut clients = self.clients.write().await;
        if let Some(state) = clients.get_mut(&client_id) {
            state.clock += 1;
            state.presence.cursor = cursor;
            Some(encode_awareness_update(
                client_id,
                state.clock,
                &state.presence,
            ))
        } else {
            None
        }
    }

    /// Apply a remote awareness update from another instance (via NATS relay).
    /// Merges into local state and returns the re-encoded update for local broadcast.
    pub async fn apply_remote_update(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let (client_id, clock, presence) = decode_awareness_update(data)?;

        let mut clients = self.clients.write().await;
        let should_apply = match clients.get(&client_id) {
            Some(existing) => clock > existing.clock,
            None => true,
        };

        if should_apply {
            let update = encode_awareness_update(client_id, clock, &presence);
            clients.insert(client_id, AwarenessClientState { clock, presence });
            Ok(update)
        } else {
            // Return existing state for this client
            Ok(data.to_vec())
        }
    }

    /// Encode full awareness state snapshot for new clients.
    pub async fn encode_full_state(&self) -> Vec<u8> {
        let clients = self.clients.read().await;
        encode_awareness_state(&clients)
    }

    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }
}

// =============================================================================
// Wire format: compatible with Yjs awareness protocol
//
// Single update:  [num_updates: u32_le][client_id: u64_le][clock: u32_le][state_len: u32_le][state_json]
// Removal:        [1u32_le][client_id: u64_le][clock: 0u32_le][0u32_le]  (empty state = offline)
// Full state:     [num_clients: u32_le][...repeated client entries...]
// =============================================================================

fn encode_awareness_update(client_id: u64, clock: u32, presence: &UserPresence) -> Vec<u8> {
    let state_json = serde_json::to_vec(presence).unwrap_or_default();
    let mut buf = Vec::with_capacity(4 + 8 + 4 + 4 + state_json.len());
    buf.extend_from_slice(&1u32.to_le_bytes()); // num_updates = 1
    buf.extend_from_slice(&client_id.to_le_bytes());
    buf.extend_from_slice(&clock.to_le_bytes());
    buf.extend_from_slice(&(state_json.len() as u32).to_le_bytes());
    buf.extend_from_slice(&state_json);
    buf
}

fn encode_awareness_removal(client_id: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + 8 + 4 + 4);
    buf.extend_from_slice(&1u32.to_le_bytes()); // num_updates = 1
    buf.extend_from_slice(&client_id.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // clock = 0
    buf.extend_from_slice(&0u32.to_le_bytes()); // state_len = 0 (offline)
    buf
}

fn encode_awareness_state(clients: &HashMap<u64, AwarenessClientState>) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(clients.len() as u32).to_le_bytes());
    for (&client_id, state) in clients.iter() {
        let state_json = serde_json::to_vec(&state.presence).unwrap_or_default();
        buf.extend_from_slice(&client_id.to_le_bytes());
        buf.extend_from_slice(&state.clock.to_le_bytes());
        buf.extend_from_slice(&(state_json.len() as u32).to_le_bytes());
        buf.extend_from_slice(&state_json);
    }
    buf
}

fn decode_awareness_update(data: &[u8]) -> anyhow::Result<(u64, u32, UserPresence)> {
    if data.len() < 20 {
        anyhow::bail!("awareness update too short");
    }

    // Skip num_updates (first 4 bytes), read first entry
    let client_id = u64::from_le_bytes(data[4..12].try_into()?);
    let clock = u32::from_le_bytes(data[12..16].try_into()?);
    let state_len = u32::from_le_bytes(data[16..20].try_into()?) as usize;

    if state_len == 0 {
        anyhow::bail!("awareness removal, not an update");
    }

    if data.len() < 20 + state_len {
        anyhow::bail!("awareness update truncated");
    }

    let presence: UserPresence = serde_json::from_slice(&data[20..20 + state_len])?;
    Ok((client_id, clock, presence))
}

/// Decode an awareness message from a WebSocket client.
/// Extracts cursor/selection data only (identity is server-authoritative).
pub fn decode_client_awareness_cursor(data: &[u8]) -> Option<Option<CursorPosition>> {
    // data starts after the MSG_AWARENESS byte (already stripped by caller)
    if data.len() < 20 {
        return None;
    }

    // Skip num_updates (4 bytes) + client_id (8 bytes) + clock (4 bytes)
    let state_len = u32::from_le_bytes(data[16..20].try_into().ok()?) as usize;
    if state_len == 0 || data.len() < 20 + state_len {
        return None;
    }

    // Parse the JSON state — we only care about the cursor field
    let value: serde_json::Value = serde_json::from_slice(&data[20..20 + state_len]).ok()?;
    let cursor = value
        .get("cursor")
        .and_then(|c| serde_json::from_value::<CursorPosition>(c.clone()).ok());

    Some(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::awareness::derive_user_color;

    fn make_connected_user(id: &str, name: &str) -> ConnectedUser {
        ConnectedUser {
            user_id: id.to_string(),
            name: name.to_string(),
            email: format!("{}@example.com", name.to_lowercase()),
            color: derive_user_color(id),
        }
    }

    #[tokio::test]
    async fn test_add_client_assigns_unique_ids() {
        let manager = DocumentAwarenessManager::new("doc-1".to_string());
        let user1 = make_connected_user("user-1", "Alice");
        let user2 = make_connected_user("user-2", "Bob");

        let (id1, _) = manager.add_client(&user1).await;
        let (id2, _) = manager.add_client(&user2).await;

        assert_ne!(id1, id2);
        assert_eq!(manager.client_count().await, 2);
    }

    #[tokio::test]
    async fn test_remove_client_returns_removal_update() {
        let manager = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");

        let (client_id, _) = manager.add_client(&user).await;
        let removal = manager.remove_client(client_id).await;

        assert!(removal.is_some());
        let data = removal.unwrap();
        // Verify it's a valid removal (state_len = 0)
        let state_len = u32::from_le_bytes(data[16..20].try_into().unwrap());
        assert_eq!(state_len, 0);
        assert_eq!(manager.client_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_nonexistent_client_returns_none() {
        let manager = DocumentAwarenessManager::new("doc-1".to_string());
        assert!(manager.remove_client(999).await.is_none());
    }

    #[tokio::test]
    async fn test_cursor_update_preserves_server_identity() {
        let manager = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");

        let (client_id, _) = manager.add_client(&user).await;

        let cursor = CursorPosition {
            index: 5,
            length: 3,
        };
        let update = manager
            .apply_client_cursor_update(client_id, Some(cursor.clone()))
            .await
            .unwrap();

        // Decode and verify server identity is preserved
        let (_, _, presence) = decode_awareness_update(&update).unwrap();
        assert_eq!(presence.user_id, "user-1");
        assert_eq!(presence.name, "Alice");
        assert_eq!(presence.cursor, Some(cursor));
    }

    #[tokio::test]
    async fn test_encode_full_state_includes_all_clients() {
        let manager = DocumentAwarenessManager::new("doc-1".to_string());
        let user1 = make_connected_user("user-1", "Alice");
        let user2 = make_connected_user("user-2", "Bob");

        manager.add_client(&user1).await;
        manager.add_client(&user2).await;

        let state = manager.encode_full_state().await;
        // First 4 bytes = number of clients
        let num_clients = u32::from_le_bytes(state[0..4].try_into().unwrap());
        assert_eq!(num_clients, 2);
    }

    #[tokio::test]
    async fn test_client_count() {
        let manager = DocumentAwarenessManager::new("doc-1".to_string());
        assert_eq!(manager.client_count().await, 0);

        let user = make_connected_user("user-1", "Alice");
        let (id, _) = manager.add_client(&user).await;
        assert_eq!(manager.client_count().await, 1);

        manager.remove_client(id).await;
        assert_eq!(manager.client_count().await, 0);
    }

    #[test]
    fn test_decode_client_awareness_cursor_rejects_short_data() {
        // Less than 20 bytes should return None, not panic
        assert_eq!(decode_client_awareness_cursor(&[]), None);
        assert_eq!(decode_client_awareness_cursor(&[0; 15]), None);
        assert_eq!(decode_client_awareness_cursor(&[0; 16]), None);
        assert_eq!(decode_client_awareness_cursor(&[0; 19]), None);
    }

    #[test]
    fn test_decode_client_awareness_cursor_with_valid_cursor() {
        let cursor = CursorPosition {
            index: 10,
            length: 5,
        };
        let state = serde_json::json!({ "cursor": cursor });
        let state_bytes = serde_json::to_vec(&state).unwrap();

        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes()); // num_updates
        data.extend_from_slice(&42u64.to_le_bytes()); // client_id
        data.extend_from_slice(&1u32.to_le_bytes()); // clock
        data.extend_from_slice(&(state_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(&state_bytes);

        let result = decode_client_awareness_cursor(&data);
        assert_eq!(result, Some(Some(cursor)));
    }

    #[test]
    fn test_decode_client_awareness_cursor_without_cursor_field() {
        let state = serde_json::json!({ "name": "Alice" });
        let state_bytes = serde_json::to_vec(&state).unwrap();

        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&42u64.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(state_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(&state_bytes);

        let result = decode_client_awareness_cursor(&data);
        assert_eq!(result, Some(None));
    }

    #[test]
    fn test_decode_client_awareness_cursor_empty_state() {
        // state_len = 0 should return None
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&42u64.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // state_len = 0

        assert_eq!(decode_client_awareness_cursor(&data), None);
    }

    #[tokio::test]
    async fn test_apply_remote_update() {
        let manager = DocumentAwarenessManager::new("doc-1".to_string());

        // Create a remote update
        let presence = UserPresence {
            user_id: "remote-user".to_string(),
            name: "Remote".to_string(),
            email: "remote@example.com".to_string(),
            color: "#ff0000".to_string(),
            cursor: None,
        };
        let remote_update = encode_awareness_update(100, 1, &presence);

        let result = manager.apply_remote_update(&remote_update).await;
        assert!(result.is_ok());
        assert_eq!(manager.client_count().await, 1);
    }
}
