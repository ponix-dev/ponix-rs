use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::domain::awareness::UserPresence;
use crate::domain::connected_user::ConnectedUser;

/// Hybrid awareness manager: uses the client's Yjs clientID but enforces
/// server-authoritative identity (user_id, name, email, color).
///
/// When a client sends an awareness update, the server extracts the cursor/selection
/// data and merges it with the server-authoritative identity fields. This prevents
/// clients from spoofing their identity while preserving the client's Yjs clientID
/// for correct cursor rendering.
pub struct DocumentAwarenessManager {
    document_id: String,
    /// Yjs clientID → stored awareness entry (with merged server-authoritative state)
    entries: RwLock<HashMap<u64, StoredAwareness>>,
    /// Room client_id → registered client identity and Yjs clientID mapping
    registered_clients: RwLock<HashMap<u64, RegisteredClient>>,
}

struct RegisteredClient {
    /// Server-authoritative identity (no cursor — cursor comes from client)
    presence: UserPresence,
    /// Set when the first awareness update arrives from the client
    yjs_client_id: Option<u64>,
}

struct StoredAwareness {
    /// Raw per-entry bytes: [clientID varint][clock varint][state var_string]
    entry_bytes: Vec<u8>,
    /// Parsed clock for removal generation
    clock: u64,
}

impl DocumentAwarenessManager {
    pub fn new(document_id: String) -> Self {
        Self {
            document_id,
            entries: RwLock::new(HashMap::new()),
            registered_clients: RwLock::new(HashMap::new()),
        }
    }

    /// Register a client's server-authoritative identity. Called when a WebSocket
    /// client connects and authenticates. The identity is merged into awareness
    /// updates when the client sends them.
    pub async fn register_client(&self, room_client_id: u64, user: &ConnectedUser) {
        self.registered_clients.write().await.insert(
            room_client_id,
            RegisteredClient {
                presence: user.to_presence(),
                yjs_client_id: None,
            },
        );
    }

    /// Process a client's raw awareness update. Extracts cursor data from the client's
    /// message, merges with server-authoritative identity, and returns the merged
    /// awareness bytes for relay to other clients.
    ///
    /// Returns `Some(merged_bytes)` for relay, or `None` if the update can't be processed.
    pub async fn store_client_update(
        &self,
        room_client_id: u64,
        raw_data: &[u8],
    ) -> Option<Vec<u8>> {
        let parsed = parse_awareness_entries(raw_data)?;
        let (yjs_client_id, clock, _) = parsed.first()?;
        let yjs_client_id = *yjs_client_id;
        let clock = *clock;

        // Extract client's state JSON (contains cursor, selection, data, etc.)
        let client_state = extract_state_object(raw_data);

        tracing::debug!(
            document_id = %self.document_id,
            yjs_client_id,
            clock,
            has_selection = client_state.as_ref().is_some_and(|s| s.contains_key("selection")),
            has_data = client_state.as_ref().is_some_and(|s| s.contains_key("data")),
            client_keys = ?client_state.as_ref().map(|s| s.keys().collect::<Vec<_>>()),
            "client awareness state received"
        );

        // Get server-authoritative identity and update Yjs clientID mapping
        let mut clients = self.registered_clients.write().await;
        let client = clients.get_mut(&room_client_id)?;
        client.yjs_client_id = Some(yjs_client_id);

        // Merge: start with client state, overwrite identity fields with server-authoritative values.
        // This preserves client-owned fields (selection, data, cursor) while preventing spoofing.
        let merged_state = merge_client_state(client_state, &client.presence);

        // Encode merged state as Yjs awareness update
        let merged_update = encode_yjs_awareness_json(yjs_client_id, clock, &merged_state);

        // Parse the merged update to extract per-entry bytes for storage
        drop(clients);
        if let Some(entries) = parse_awareness_entries(&merged_update) {
            if let Some((_, _, entry_bytes)) = entries.into_iter().next() {
                self.entries
                    .write()
                    .await
                    .insert(yjs_client_id, StoredAwareness { entry_bytes, clock });
            }
        }

        Some(merged_update)
    }

    /// Store a remote awareness update from NATS (another server instance).
    /// Remote updates are already merged by their origin server, so we store as-is.
    pub async fn store_remote_update(&self, raw_data: &[u8]) {
        if let Some(entries) = parse_awareness_entries(raw_data) {
            let mut stored = self.entries.write().await;
            for (yjs_client_id, clock, entry_bytes) in entries {
                if is_removal_entry(&entry_bytes) {
                    stored.remove(&yjs_client_id);
                } else {
                    let should_apply = match stored.get(&yjs_client_id) {
                        Some(existing) => clock >= existing.clock,
                        None => true,
                    };
                    if should_apply {
                        stored.insert(yjs_client_id, StoredAwareness { entry_bytes, clock });
                    }
                }
            }
        }
    }

    /// Remove a local client. Returns the encoded Yjs removal update for broadcasting.
    pub async fn remove_client(&self, room_client_id: u64) -> Option<Vec<u8>> {
        let client = self
            .registered_clients
            .write()
            .await
            .remove(&room_client_id)?;
        let yjs_client_id = client.yjs_client_id?;
        let stored = self.entries.write().await.remove(&yjs_client_id)?;
        Some(encode_yjs_removal(yjs_client_id, stored.clock + 1))
    }

    /// Encode full awareness state for a new client.
    /// Returns a Yjs-compatible awareness update containing all known entries.
    pub async fn encode_full_state(&self) -> Vec<u8> {
        let entries = self.entries.read().await;
        let mut buf = Vec::new();
        write_var_uint(&mut buf, entries.len() as u64);
        for entry in entries.values() {
            buf.extend_from_slice(&entry.entry_bytes);
        }
        buf
    }

    pub async fn client_count(&self) -> usize {
        self.entries.read().await.len()
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }
}

// =============================================================================
// Yjs awareness wire format (lib0 varint encoding)
//
// Awareness update: [count: varint]([clientID: varint][clock: varint][state: var_string])*
// Removal: count=1, state = "null" (JSON-encoded null)
// var_string: [byte_length: varint][utf8_bytes]
// varint: 7 bits per byte, MSB = continuation, little-endian order
// =============================================================================

/// Parse an awareness update into individual entries.
/// Returns Vec<(yjs_client_id, clock, raw_entry_bytes)>.
fn parse_awareness_entries(data: &[u8]) -> Option<Vec<(u64, u64, Vec<u8>)>> {
    let (count, mut pos) = read_var_uint(data, 0)?;
    let mut entries = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let entry_start = pos;
        let (client_id, new_pos) = read_var_uint(data, pos)?;
        let (clock, new_pos) = read_var_uint(data, new_pos)?;
        let (str_len, new_pos) = read_var_uint(data, new_pos)?;
        let str_end = new_pos + str_len as usize;
        if str_end > data.len() {
            return None;
        }
        pos = str_end;
        entries.push((client_id, clock, data[entry_start..pos].to_vec()));
    }

    Some(entries)
}

/// Extract the state JSON object from the first entry in a raw awareness update.
fn extract_state_object(data: &[u8]) -> Option<serde_json::Map<String, serde_json::Value>> {
    let (_, mut pos) = read_var_uint(data, 0)?; // count
    let (_, new_pos) = read_var_uint(data, pos)?; // clientID
    pos = new_pos;
    let (_, new_pos) = read_var_uint(data, pos)?; // clock
    pos = new_pos;
    let (str_len, new_pos) = read_var_uint(data, pos)?;
    let str_end = new_pos + str_len as usize;
    if str_len == 0 || str_end > data.len() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(&data[new_pos..str_end]).ok()?;
    match value {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

/// Merge client state with server-authoritative identity.
/// Starts with client fields (selection, data, cursor, etc.) and overwrites identity fields.
fn merge_client_state(
    client_state: Option<serde_json::Map<String, serde_json::Value>>,
    presence: &UserPresence,
) -> String {
    let mut merged = client_state.unwrap_or_default();

    // Overwrite identity fields with server-authoritative values
    merged.insert("user_id".to_string(), serde_json::json!(presence.user_id));
    merged.insert("name".to_string(), serde_json::json!(presence.name));
    merged.insert("email".to_string(), serde_json::json!(presence.email));
    merged.insert("color".to_string(), serde_json::json!(presence.color));

    serde_json::to_string(&merged).unwrap_or_default()
}

/// Check if an entry represents a removal (state = "null").
fn is_removal_entry(entry_bytes: &[u8]) -> bool {
    let (_, pos) = match read_var_uint(entry_bytes, 0) {
        Some(v) => v,
        None => return false,
    };
    let (_, pos) = match read_var_uint(entry_bytes, pos) {
        Some(v) => v,
        None => return false,
    };
    let (str_len, pos) = match read_var_uint(entry_bytes, pos) {
        Some(v) => v,
        None => return false,
    };
    str_len == 4 && pos + 4 <= entry_bytes.len() && &entry_bytes[pos..pos + 4] == b"null"
}

/// Encode a Yjs awareness update with a pre-serialized JSON state string.
fn encode_yjs_awareness_json(client_id: u64, clock: u64, state_json: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    write_var_uint(&mut buf, 1); // count = 1
    write_var_uint(&mut buf, client_id);
    write_var_uint(&mut buf, clock);
    write_var_uint(&mut buf, state_json.len() as u64);
    buf.extend_from_slice(state_json.as_bytes());
    buf
}

/// Encode a Yjs awareness removal for a client.
fn encode_yjs_removal(client_id: u64, clock: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    write_var_uint(&mut buf, 1); // count = 1
    write_var_uint(&mut buf, client_id);
    write_var_uint(&mut buf, clock);
    write_var_uint(&mut buf, 4); // string length
    buf.extend_from_slice(b"null");
    buf
}

fn read_var_uint(data: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        if pos >= data.len() {
            return None;
        }
        let byte = data[pos];
        pos += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    Some((result, pos))
}

fn write_var_uint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        if value <= 0x7F {
            buf.push(value as u8);
            break;
        }
        buf.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_connected_user(id: &str, name: &str) -> ConnectedUser {
        ConnectedUser {
            user_id: id.to_string(),
            name: name.to_string(),
            email: format!("{}@example.com", name.to_lowercase()),
            color: "#ff0000".to_string(),
        }
    }

    /// Encode a Yjs awareness update with a UserPresence struct (for test remote updates).
    fn encode_yjs_awareness(client_id: u64, clock: u64, presence: &UserPresence) -> Vec<u8> {
        let state_json = serde_json::to_string(presence).unwrap_or_default();
        encode_yjs_awareness_json(client_id, clock, &state_json)
    }

    /// Helper: encode a raw Yjs awareness update as a client would send it.
    fn encode_client_awareness(client_id: u64, clock: u64, state_json: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        write_var_uint(&mut buf, 1);
        write_var_uint(&mut buf, client_id);
        write_var_uint(&mut buf, clock);
        let state_bytes = state_json.as_bytes();
        write_var_uint(&mut buf, state_bytes.len() as u64);
        buf.extend_from_slice(state_bytes);
        buf
    }

    #[tokio::test]
    async fn test_register_and_store_client_update() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");

        mgr.register_client(1, &user).await;

        // Client sends awareness with cursor data
        let client_state = r#"{"user":{"name":"Hacker"},"cursor":{"index":5}}"#;
        let raw = encode_client_awareness(3529026288, 1, client_state);

        let merged = mgr.store_client_update(1, &raw).await;
        assert!(merged.is_some());

        // Verify merged state uses server identity, not client-supplied "Hacker"
        let merged = merged.unwrap();
        let entries = parse_awareness_entries(&merged).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, 3529026288); // Client's Yjs clientID preserved

        // Parse the state JSON from the merged entry
        let state_json = extract_state_json(&merged).unwrap();
        let state: serde_json::Value = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state["user_id"], "user-1");
        assert_eq!(state["name"], "Alice");
        assert_eq!(state["email"], "alice@example.com");
        assert!(state["color"].as_str().unwrap().starts_with('#'));
        assert_eq!(state["cursor"]["index"], 5);
    }

    #[tokio::test]
    async fn test_client_cannot_spoof_identity() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");
        mgr.register_client(1, &user).await;

        // Client tries to claim they are "Bob" with a different email
        let spoofed = r#"{"user_id":"user-2","name":"Bob","email":"bob@evil.com","cursor":null}"#;
        let raw = encode_client_awareness(42, 1, spoofed);

        let merged = mgr.store_client_update(1, &raw).await.unwrap();
        let state_json = extract_state_json(&merged).unwrap();
        let state: serde_json::Value = serde_json::from_str(&state_json).unwrap();

        // Server identity wins
        assert_eq!(state["user_id"], "user-1");
        assert_eq!(state["name"], "Alice");
        assert_eq!(state["email"], "alice@example.com");
    }

    #[tokio::test]
    async fn test_full_state_includes_merged_entries() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());

        let user1 = make_connected_user("user-1", "Alice");
        let user2 = make_connected_user("user-2", "Bob");
        mgr.register_client(1, &user1).await;
        mgr.register_client(2, &user2).await;

        let raw1 = encode_client_awareness(1000, 1, r#"{"cursor":{"index":0}}"#);
        let raw2 = encode_client_awareness(2000, 1, r#"{"cursor":{"index":10}}"#);
        mgr.store_client_update(1, &raw1).await;
        mgr.store_client_update(2, &raw2).await;

        assert_eq!(mgr.client_count().await, 2);

        let full = mgr.encode_full_state().await;
        let entries = parse_awareness_entries(&full).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_client_generates_removal() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");
        mgr.register_client(1, &user).await;

        let raw = encode_client_awareness(42, 5, r#"{"cursor":null}"#);
        mgr.store_client_update(1, &raw).await;

        let removal = mgr.remove_client(1).await;
        assert!(removal.is_some());

        let removal = removal.unwrap();
        let entries = parse_awareness_entries(&removal).unwrap();
        assert_eq!(entries[0].0, 42); // Same Yjs clientID
        assert_eq!(entries[0].1, 6); // clock + 1
        assert!(is_removal_entry(&entries[0].2));

        assert_eq!(mgr.client_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_unregistered_client_returns_none() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        assert!(mgr.remove_client(999).await.is_none());
    }

    #[tokio::test]
    async fn test_remove_client_before_awareness_update() {
        // Client registers but disconnects before sending any awareness
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");
        mgr.register_client(1, &user).await;

        // No store_client_update was called, so yjs_client_id is None
        let removal = mgr.remove_client(1).await;
        assert!(removal.is_none()); // Nothing to remove
    }

    #[tokio::test]
    async fn test_store_remote_update() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());

        let presence = UserPresence {
            user_id: "remote-user".to_string(),
            name: "Remote".to_string(),
            email: "remote@example.com".to_string(),
            color: "#ff0000".to_string(),
            cursor: None,
        };
        let update = encode_yjs_awareness(100, 1, &presence);
        mgr.store_remote_update(&update).await;

        assert_eq!(mgr.client_count().await, 1);
    }

    #[tokio::test]
    async fn test_store_remote_removal_deletes_entry() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());

        let presence = UserPresence {
            user_id: "remote-user".to_string(),
            name: "Remote".to_string(),
            email: "remote@example.com".to_string(),
            color: "#ff0000".to_string(),
            cursor: None,
        };
        let update = encode_yjs_awareness(100, 1, &presence);
        mgr.store_remote_update(&update).await;
        assert_eq!(mgr.client_count().await, 1);

        let removal = encode_client_awareness(100, 2, "null");
        mgr.store_remote_update(&removal).await;
        assert_eq!(mgr.client_count().await, 0);
    }

    #[tokio::test]
    async fn test_remote_update_clock_ordering() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());

        let presence = UserPresence {
            user_id: "u".to_string(),
            name: "U".to_string(),
            email: "u@x.com".to_string(),
            color: "#000".to_string(),
            cursor: None,
        };
        let update1 = encode_yjs_awareness(100, 5, &presence);
        mgr.store_remote_update(&update1).await;

        // Older clock should not overwrite
        let update2 = encode_yjs_awareness(100, 3, &presence);
        mgr.store_remote_update(&update2).await;

        let full = mgr.encode_full_state().await;
        let entries = parse_awareness_entries(&full).unwrap();
        assert_eq!(entries[0].1, 5);
    }

    #[tokio::test]
    async fn test_cursor_update_preserves_identity() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");
        mgr.register_client(1, &user).await;

        // First update: cursor at index 0
        let raw1 = encode_client_awareness(42, 1, r#"{"cursor":{"index":0}}"#);
        mgr.store_client_update(1, &raw1).await;

        // Second update: cursor moves to index 10
        let raw2 = encode_client_awareness(42, 2, r#"{"cursor":{"index":10}}"#);
        let merged = mgr.store_client_update(1, &raw2).await.unwrap();

        let state_json = extract_state_json(&merged).unwrap();
        let state: serde_json::Value = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state["name"], "Alice"); // Identity preserved
        assert_eq!(state["cursor"]["index"], 10); // Cursor updated
    }

    #[tokio::test]
    async fn test_encode_full_state_empty() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        let full = mgr.encode_full_state().await;
        let entries = parse_awareness_entries(&full).unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_varint_roundtrip() {
        for value in [
            0u64,
            1,
            127,
            128,
            300,
            16383,
            16384,
            u32::MAX as u64,
            u64::MAX,
        ] {
            let mut buf = Vec::new();
            write_var_uint(&mut buf, value);
            let (decoded, _) = read_var_uint(&buf, 0).unwrap();
            assert_eq!(decoded, value, "varint roundtrip failed for {}", value);
        }
    }

    #[test]
    fn test_parse_awareness_entries_truncated() {
        assert!(parse_awareness_entries(&[]).is_none());
        let mut data = Vec::new();
        write_var_uint(&mut data, 1);
        assert!(parse_awareness_entries(&data).is_none());
    }

    #[test]
    fn test_is_removal_entry() {
        let full = encode_client_awareness(42, 0, "null");
        let entries = parse_awareness_entries(&full).unwrap();
        assert!(is_removal_entry(&entries[0].2));

        let full = encode_client_awareness(42, 1, r#"{"user":"Alice"}"#);
        let entries = parse_awareness_entries(&full).unwrap();
        assert!(!is_removal_entry(&entries[0].2));
    }

    #[test]
    fn test_extract_state_object() {
        let raw =
            encode_client_awareness(42, 1, r#"{"selection":{"anchor":{}},"data":{"style":{}}}"#);
        let obj = extract_state_object(&raw);
        assert!(obj.is_some());
        let obj = obj.unwrap();
        assert!(obj.contains_key("selection"));
        assert!(obj.contains_key("data"));

        // Non-object state
        let raw = encode_client_awareness(42, 1, "null");
        assert!(extract_state_object(&raw).is_none());
    }

    #[tokio::test]
    async fn test_merge_preserves_selection_and_data() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");
        mgr.register_client(1, &user).await;

        // Client sends awareness with selection and data (withCursors format)
        let client_state = r##"{"data":{"style":{"backgroundColor":"#363fc233"},"selectionStyle":{"backgroundColor":"#363fc233"}},"selection":{"anchor":{"path":[0,0],"offset":3},"focus":{"path":[0,0],"offset":3}}}"##;
        let raw = encode_client_awareness(42, 1, client_state);

        let merged = mgr.store_client_update(1, &raw).await.unwrap();
        let state_json = extract_state_json(&merged).unwrap();
        let state: serde_json::Value = serde_json::from_str(&state_json).unwrap();

        // Server identity present
        assert_eq!(state["user_id"], "user-1");
        assert_eq!(state["name"], "Alice");
        assert_eq!(state["email"], "alice@example.com");
        assert!(state["color"].as_str().unwrap().starts_with('#'));

        // Client fields preserved
        assert_eq!(
            state["selection"]["anchor"]["path"],
            serde_json::json!([0, 0])
        );
        assert_eq!(state["selection"]["anchor"]["offset"], 3);
        assert!(state["data"]["style"]["backgroundColor"].is_string());
        assert!(state["data"]["selectionStyle"]["backgroundColor"].is_string());
    }

    #[tokio::test]
    async fn test_merge_preserves_yjs_relative_position_selection() {
        let mgr = DocumentAwarenessManager::new("doc-1".to_string());
        let user = make_connected_user("user-1", "Alice");
        mgr.register_client(1, &user).await;

        // Exact format from withCursors — Yjs RelativePosition objects
        let client_state = r##"{"data":{"style":{"backgroundColor":"#363fc233"},"selectionStyle":{"backgroundColor":"#363fc233"}},"selection":{"anchor":{"type":{"client":3013080589,"clock":3},"tname":null,"item":null,"assoc":-1},"focus":{"type":{"client":3013080589,"clock":0},"tname":null,"item":{"client":3013080589,"clock":76},"assoc":0}}}"##;
        let raw = encode_client_awareness(42, 1, client_state);

        let merged = mgr.store_client_update(1, &raw).await.unwrap();
        let state_json = extract_state_json(&merged).unwrap();
        let state: serde_json::Value = serde_json::from_str(&state_json).unwrap();

        // Server identity present
        assert_eq!(state["name"], "Alice");

        // selection with Yjs RelativePosition preserved exactly
        assert_eq!(
            state["selection"]["anchor"]["type"]["client"],
            3013080589u64
        );
        assert_eq!(state["selection"]["anchor"]["type"]["clock"], 3);
        assert!(state["selection"]["anchor"]["tname"].is_null());
        assert!(state["selection"]["anchor"]["item"].is_null());
        assert_eq!(state["selection"]["anchor"]["assoc"], -1);
        assert_eq!(state["selection"]["focus"]["item"]["client"], 3013080589u64);
        assert_eq!(state["selection"]["focus"]["item"]["clock"], 76);
        assert_eq!(state["selection"]["focus"]["assoc"], 0);

        // data preserved
        assert!(state["data"]["style"]["backgroundColor"].is_string());
    }

    /// Helper: extract the state JSON string from the first entry of an awareness update.
    fn extract_state_json(data: &[u8]) -> Option<String> {
        let (_, mut pos) = read_var_uint(data, 0)?; // count
        let (_, new_pos) = read_var_uint(data, pos)?; // clientID
        pos = new_pos;
        let (_, new_pos) = read_var_uint(data, pos)?; // clock
        pos = new_pos;
        let (str_len, new_pos) = read_var_uint(data, pos)?;
        let str_end = new_pos + str_len as usize;
        if str_end > data.len() {
            return None;
        }
        Some(String::from_utf8_lossy(&data[new_pos..str_end]).to_string())
    }
}
