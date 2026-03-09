#![cfg(feature = "integration-tests")]

use std::sync::Arc;
use std::time::Duration;

use collaboration_server::domain::RoomManager;
use collaboration_server::domain::{decode_sync_message, encode_update, SyncMessage};
use collaboration_server::nats::NatsDocumentRelay;
use collaboration_server::websocket::{build_router, AppState};
use common::auth::AuthTokenProvider;
use common::domain::{
    CreateDocumentRepoInputWithId, Document, DocumentRepository, DomainResult, GetDocumentRepoInput,
};
use common::yrs::create_empty_document;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite;
use yrs::updates::decoder::Decode;
use yrs::{GetString, Text, Transact};

/// Simple in-memory document repository for integration tests
struct InMemoryDocumentRepository {
    documents: tokio::sync::RwLock<Vec<Document>>,
}

impl InMemoryDocumentRepository {
    fn new() -> Self {
        Self {
            documents: tokio::sync::RwLock::new(Vec::new()),
        }
    }

    async fn insert(&self, doc: Document) {
        self.documents.write().await.push(doc);
    }
}

#[async_trait::async_trait]
impl DocumentRepository for InMemoryDocumentRepository {
    async fn create_document(
        &self,
        _input: CreateDocumentRepoInputWithId,
    ) -> DomainResult<Document> {
        unimplemented!()
    }

    async fn get_document(&self, _input: GetDocumentRepoInput) -> DomainResult<Option<Document>> {
        unimplemented!()
    }

    async fn get_document_by_id(&self, document_id: &str) -> DomainResult<Option<Document>> {
        let docs = self.documents.read().await;
        Ok(docs.iter().find(|d| d.document_id == document_id).cloned())
    }

    async fn update_document(
        &self,
        _input: common::domain::UpdateDocumentRepoInput,
    ) -> DomainResult<Document> {
        unimplemented!()
    }

    async fn delete_document(
        &self,
        _input: common::domain::DeleteDocumentRepoInput,
    ) -> DomainResult<()> {
        unimplemented!()
    }

    async fn list_documents(
        &self,
        _input: common::domain::ListDocumentsRepoInput,
    ) -> DomainResult<Vec<Document>> {
        unimplemented!()
    }

    async fn update_yrs_state(
        &self,
        _input: common::domain::UpdateYrsStateInput,
    ) -> DomainResult<bool> {
        unimplemented!()
    }
}

/// Simple auth provider that accepts "valid-token" and rejects everything else
struct TestAuthTokenProvider;

impl AuthTokenProvider for TestAuthTokenProvider {
    fn generate_token(&self, user_id: &str, _email: &str) -> DomainResult<String> {
        Ok(format!("token-for-{}", user_id))
    }

    fn validate_token(&self, token: &str) -> DomainResult<String> {
        if token == "valid-token" {
            Ok("test-user-id".to_string())
        } else {
            Err(common::domain::DomainError::InvalidToken(
                "invalid token".to_string(),
            ))
        }
    }

    fn extract_user_id(&self, _token: &str) -> Option<String> {
        Some("test-user-id".to_string())
    }
}

async fn setup_test_server() -> (String, Arc<InMemoryDocumentRepository>) {
    let doc_repo = Arc::new(InMemoryDocumentRepository::new());
    let nats_relay = Arc::new(NatsDocumentRelay::new_stub());
    let auth_provider: Arc<dyn AuthTokenProvider> = Arc::new(TestAuthTokenProvider);

    let room_manager = Arc::new(RoomManager::new(
        doc_repo.clone() as Arc<dyn DocumentRepository>,
        nats_relay.clone(),
        "document_sync".to_string(),
    ));

    let app_state = Arc::new(AppState {
        room_manager,
        nats_relay,
        auth_token_provider: auth_provider,
    });

    let router = build_router(app_state, "*");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("ws://127.0.0.1:{}", addr.port()), doc_repo)
}

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

#[tokio::test]
async fn test_connect_and_auth() {
    let (ws_url, doc_repo) = setup_test_server().await;

    // Insert a document
    doc_repo.insert(make_test_document("doc-1")).await;

    // Connect via WebSocket
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("{}/ws/documents/doc-1", ws_url))
        .await
        .expect("Failed to connect");

    // Send JWT as first message
    ws.send(tungstenite::Message::Text("valid-token".into()))
        .await
        .unwrap();

    // Should receive SyncStep1 (binary)
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match msg {
        tungstenite::Message::Binary(data) => {
            let decoded = decode_sync_message(&data).unwrap();
            assert!(matches!(decoded, Some(SyncMessage::SyncStep1(_))));
        }
        other => panic!("Expected binary message, got: {:?}", other),
    }

    // Should receive SyncStep2 (binary)
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match msg {
        tungstenite::Message::Binary(data) => {
            let decoded = decode_sync_message(&data).unwrap();
            assert!(matches!(decoded, Some(SyncMessage::SyncStep2(_))));
        }
        other => panic!("Expected binary message, got: {:?}", other),
    }

    ws.close(None).await.unwrap();
}

#[tokio::test]
async fn test_reject_nonexistent_document() {
    let (ws_url, _doc_repo) = setup_test_server().await;

    // Try to connect to a non-existent document
    let result =
        tokio_tungstenite::connect_async(format!("{}/ws/documents/nonexistent", ws_url)).await;

    // Should get a connection error (server returns 404)
    assert!(result.is_err());
}

#[tokio::test]
async fn test_reject_invalid_auth() {
    let (ws_url, doc_repo) = setup_test_server().await;

    doc_repo.insert(make_test_document("doc-auth")).await;

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("{}/ws/documents/doc-auth", ws_url))
        .await
        .expect("Failed to connect");

    // Send invalid token
    ws.send(tungstenite::Message::Text("bad-token".into()))
        .await
        .unwrap();

    // Should receive a close frame
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match msg {
        tungstenite::Message::Close(Some(frame)) => {
            assert_eq!(
                frame.code,
                tungstenite::protocol::frame::coding::CloseCode::from(4001)
            );
            assert!(frame.reason.contains("invalid"));
        }
        tungstenite::Message::Close(None) => {
            // Also acceptable — server closed connection
        }
        other => panic!("Expected close message, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_two_client_sync() {
    let (ws_url, doc_repo) = setup_test_server().await;

    doc_repo.insert(make_test_document("doc-sync")).await;

    // Connect client A
    let (mut ws_a, _) =
        tokio_tungstenite::connect_async(format!("{}/ws/documents/doc-sync", ws_url))
            .await
            .unwrap();

    ws_a.send(tungstenite::Message::Text("valid-token".into()))
        .await
        .unwrap();

    // Drain initial sync messages (SyncStep1 + SyncStep2)
    let _ = tokio::time::timeout(Duration::from_secs(2), ws_a.next()).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), ws_a.next()).await;

    // Connect client B
    let (mut ws_b, _) =
        tokio_tungstenite::connect_async(format!("{}/ws/documents/doc-sync", ws_url))
            .await
            .unwrap();

    ws_b.send(tungstenite::Message::Text("valid-token".into()))
        .await
        .unwrap();

    // Drain initial sync messages for B
    let _ = tokio::time::timeout(Duration::from_secs(2), ws_b.next()).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), ws_b.next()).await;

    // Client A sends an update
    let doc = yrs::Doc::new();
    let (state, _) = create_empty_document();
    {
        let mut txn = doc.transact_mut();
        let update = yrs::Update::decode_v1(&state).unwrap();
        txn.apply_update(update).unwrap();
    }
    let text = doc.get_or_insert_text("content");
    let update_bytes = {
        let mut txn = doc.transact_mut();
        text.insert(&mut txn, 0, "hello from A");
        txn.encode_update_v1()
    };

    let sync_update = encode_update(&update_bytes);
    ws_a.send(tungstenite::Message::Binary(sync_update.into()))
        .await
        .unwrap();

    // Client B should receive the update and be able to apply it
    let msg = tokio::time::timeout(Duration::from_secs(5), ws_b.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match msg {
        tungstenite::Message::Binary(data) => {
            let decoded = decode_sync_message(&data).unwrap();
            match decoded {
                Some(SyncMessage::Update(update_data)) => {
                    // Apply the received update to a fresh doc and verify content
                    let doc_b = yrs::Doc::new();
                    let (initial_state, _) = create_empty_document();
                    {
                        let mut txn = doc_b.transact_mut();
                        let u = yrs::Update::decode_v1(&initial_state).unwrap();
                        txn.apply_update(u).unwrap();
                    }
                    {
                        let mut txn = doc_b.transact_mut();
                        let u = yrs::Update::decode_v1(&update_data).unwrap();
                        txn.apply_update(u).unwrap();
                    }
                    let text_b = doc_b.get_or_insert_text("content");
                    let txn = doc_b.transact();
                    assert_eq!(
                        text_b.get_string(&txn),
                        "hello from A",
                        "Client B should see the text written by Client A"
                    );
                }
                other => panic!("Expected SyncMessage::Update, got: {:?}", other),
            }
        }
        other => panic!("Expected binary update message, got: {:?}", other),
    }

    ws_a.close(None).await.unwrap();
    ws_b.close(None).await.unwrap();
}

#[tokio::test]
async fn test_client_disconnect_cleanup() {
    let (ws_url, doc_repo) = setup_test_server().await;

    doc_repo.insert(make_test_document("doc-cleanup")).await;

    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("{}/ws/documents/doc-cleanup", ws_url))
            .await
            .unwrap();

    ws.send(tungstenite::Message::Text("valid-token".into()))
        .await
        .unwrap();

    // Drain initial sync
    let _ = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;

    // Disconnect
    ws.close(None).await.unwrap();

    // Give server time to clean up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Room should be destroyed — reconnecting should work (room recreated from PG)
    let (mut ws2, _) =
        tokio_tungstenite::connect_async(format!("{}/ws/documents/doc-cleanup", ws_url))
            .await
            .unwrap();

    ws2.send(tungstenite::Message::Text("valid-token".into()))
        .await
        .unwrap();

    // Should still get sync messages
    let msg = tokio::time::timeout(Duration::from_secs(5), ws2.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert!(matches!(msg, tungstenite::Message::Binary(_)));

    ws2.close(None).await.unwrap();
}

#[tokio::test]
async fn test_new_client_receives_existing_content() {
    let (ws_url, doc_repo) = setup_test_server().await;

    doc_repo.insert(make_test_document("doc-content")).await;

    // Client A connects and writes content
    let (mut ws_a, _) =
        tokio_tungstenite::connect_async(format!("{}/ws/documents/doc-content", ws_url))
            .await
            .unwrap();

    ws_a.send(tungstenite::Message::Text("valid-token".into()))
        .await
        .unwrap();

    // Drain initial sync messages (SyncStep1 + SyncStep2)
    let _ = tokio::time::timeout(Duration::from_secs(2), ws_a.next()).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), ws_a.next()).await;

    // Create a Yrs update with content
    let doc_a = yrs::Doc::new();
    let (state, _) = create_empty_document();
    {
        let mut txn = doc_a.transact_mut();
        let update = yrs::Update::decode_v1(&state).unwrap();
        txn.apply_update(update).unwrap();
    }
    let text_a = doc_a.get_or_insert_text("content");
    let update_bytes = {
        let mut txn = doc_a.transact_mut();
        text_a.insert(&mut txn, 0, "persistent content");
        txn.encode_update_v1()
    };

    // Send the update
    let sync_update = encode_update(&update_bytes);
    ws_a.send(tungstenite::Message::Binary(sync_update.into()))
        .await
        .unwrap();

    // Give server time to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client B connects — should receive full state including A's content via SyncStep2
    let (mut ws_b, _) =
        tokio_tungstenite::connect_async(format!("{}/ws/documents/doc-content", ws_url))
            .await
            .unwrap();

    ws_b.send(tungstenite::Message::Text("valid-token".into()))
        .await
        .unwrap();

    // Receive SyncStep1 (state vector)
    let _ = tokio::time::timeout(Duration::from_secs(5), ws_b.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Receive SyncStep2 (full document state)
    let step2_msg = tokio::time::timeout(Duration::from_secs(5), ws_b.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match step2_msg {
        tungstenite::Message::Binary(data) => {
            let decoded = decode_sync_message(&data).unwrap();
            match decoded {
                Some(SyncMessage::SyncStep2(full_state)) => {
                    // Apply the full state to a fresh doc and verify content
                    let doc_b = yrs::Doc::new();
                    {
                        let mut txn = doc_b.transact_mut();
                        let u = yrs::Update::decode_v1(&full_state).unwrap();
                        txn.apply_update(u).unwrap();
                    }
                    let text_b = doc_b.get_or_insert_text("content");
                    let txn = doc_b.transact();
                    assert_eq!(
                        text_b.get_string(&txn),
                        "persistent content",
                        "New client should receive existing content via initial sync"
                    );
                }
                other => panic!("Expected SyncStep2, got: {:?}", other),
            }
        }
        other => panic!("Expected binary message, got: {:?}", other),
    }

    ws_a.close(None).await.unwrap();
    ws_b.close(None).await.unwrap();
}
