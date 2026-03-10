use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use common::auth::AuthTokenProvider;
use common::domain::UserRepository;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::domain::connected_user::ConnectedUser;
use crate::domain::protocol::MSG_AWARENESS;
use crate::domain::{
    decode_client_awareness_cursor, decode_sync_message, encode_sync_step1, encode_sync_step2,
    SyncMessage,
};
use crate::domain::{DocumentRoom, RoomManager};
use crate::nats::{AwarenessRelay, NatsDocumentRelay};
use crate::websocket::auth::authenticate_first_message;

pub async fn handle_connection(
    mut socket: WebSocket,
    room: Arc<DocumentRoom>,
    room_manager: Arc<RoomManager>,
    nats_relay: Arc<NatsDocumentRelay>,
    awareness_relay: Option<Arc<AwarenessRelay>>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
    user_repository: Arc<dyn UserRepository>,
) {
    let document_id = room.document_id().to_string();

    // 1. First-message auth: wait for JWT as first text message
    let authenticated_user_id =
        match authenticate_first_message(&mut socket, auth_token_provider.as_ref()).await {
            Ok(user_id) => user_id,
            Err(e) => {
                tracing::warn!(
                    document_id = %document_id,
                    reason = e.close_reason(),
                    "WebSocket auth failed"
                );
                let _ = socket
                    .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: 4001,
                        reason: e.close_reason().into(),
                    })))
                    .await;
                // Clean up room if no clients
                if room.client_count().await == 0 {
                    nats_relay.unsubscribe(&document_id).await;
                    room_manager.remove_room(&document_id).await;
                }
                return;
            }
        };

    // 2. Resolve full user identity for awareness (server-authoritative)
    let connected_user =
        match ConnectedUser::from_user_id(&authenticated_user_id, user_repository.as_ref()).await {
            Ok(user) => user,
            Err(e) => {
                tracing::warn!(
                    document_id = %document_id,
                    error = %e,
                    "failed to resolve user identity"
                );
                let _ = socket
                    .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: 4001,
                        reason: "failed to resolve user identity".into(),
                    })))
                    .await;
                if room.client_count().await == 0 {
                    nats_relay.unsubscribe(&document_id).await;
                    room_manager.remove_room(&document_id).await;
                }
                return;
            }
        };

    let user_id = connected_user.user_id.clone();
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // 3. Create channel for outbound messages (room broadcasts to us via this)
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(256);

    // 4. Add client to room — get initial sync data
    let (client_id, state_vector, full_update) = room.add_client(user_id.clone(), tx.clone()).await;

    tracing::info!(
        document_id = %document_id,
        user_id = %user_id,
        client_id = client_id,
        "client authenticated and connected"
    );

    // 5. Send initial sync: SyncStep1 (our state vector) + SyncStep2 (full state)
    let step1 = encode_sync_step1(&state_vector);
    let step2 = encode_sync_step2(&full_update);
    if ws_sender.send(Message::Binary(step1.into())).await.is_err()
        || ws_sender.send(Message::Binary(step2.into())).await.is_err()
    {
        room.remove_client(client_id).await;
        return;
    }

    // 6. Set up awareness
    let awareness_manager = room_manager.get_or_create_awareness(&document_id).await;
    let (awareness_client_id, awareness_add_update) =
        awareness_manager.add_client(&connected_user).await;

    // Send full awareness state to new client
    let full_awareness = awareness_manager.encode_full_state().await;
    let awareness_msg = encode_awareness_message(&full_awareness);
    if ws_sender
        .send(Message::Binary(awareness_msg.into()))
        .await
        .is_err()
    {
        awareness_manager.remove_client(awareness_client_id).await;
        room.remove_client(client_id).await;
        return;
    }

    // Broadcast new user's presence to other local clients
    broadcast_awareness_to_room(&room, client_id, &awareness_add_update).await;

    // Publish to NATS for cross-instance relay
    if let Some(relay) = &awareness_relay {
        if let Err(e) = relay.publish(&document_id, &awareness_add_update).await {
            tracing::warn!(error = %e, "failed to publish awareness to NATS");
        }
    }

    // 7. Start NATS awareness subscription relay (if first client needs it)
    let awareness_relay_clone = awareness_relay.clone();
    let awareness_mgr_clone = awareness_manager.clone();
    let room_clone = room.clone();
    let awareness_sub_cancel = tokio_util::sync::CancellationToken::new();
    let awareness_sub_cancel_clone = awareness_sub_cancel.clone();

    if let Some(relay) = &awareness_relay_clone {
        if let Ok(mut subscriber) = relay.subscribe(&document_id).await {
            let relay_for_sub = relay.clone();
            let cancel = awareness_sub_cancel_clone.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        msg = subscriber.next() => {
                            let Some(msg) = msg else { break };
                            if relay_for_sub.is_from_self(&msg) {
                                continue;
                            }
                            match awareness_mgr_clone.apply_remote_update(&msg.payload).await {
                                Ok(update) => {
                                    let encoded = encode_awareness_message(&update);
                                    let clients = room_clone.clients_read().await;
                                    for client in clients.values() {
                                        let _ = client.sender.try_send(encoded.clone());
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "failed to apply remote awareness update");
                                }
                            }
                        }
                    }
                }
            });
        }
    }

    // Spawn outbound task: room broadcasts -> WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if ws_sender.send(Message::Binary(data.into())).await.is_err() {
                break;
            }
        }
    });

    // Inbound loop: WebSocket -> room + NATS
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Binary(data) => {
                if data.is_empty() {
                    continue;
                }

                if data[0] == MSG_AWARENESS {
                    // Awareness message from client
                    let awareness_data = &data[1..];
                    if let Some(cursor) = decode_client_awareness_cursor(awareness_data) {
                        if let Some(update) = awareness_manager
                            .apply_client_cursor_update(awareness_client_id, cursor)
                            .await
                        {
                            // Broadcast to other local clients
                            broadcast_awareness_to_room(&room, client_id, &update).await;

                            // Relay to other instances
                            if let Some(relay) = &awareness_relay {
                                if let Err(e) = relay.publish(&document_id, &update).await {
                                    tracing::warn!(error = %e, "failed to publish awareness to NATS");
                                }
                            }
                        }
                    }
                    continue;
                }

                match decode_sync_message(&data) {
                    Ok(Some(SyncMessage::SyncStep1(sv))) => {
                        // Client sent their state vector — compute and send diff
                        match room.compute_diff(&sv).await {
                            Ok(diff) => {
                                let response = encode_sync_step2(&diff);
                                room.send_to_client(client_id, response).await;
                            }
                            Err(e) => tracing::warn!(error = %e, "failed to compute diff"),
                        }
                    }
                    Ok(Some(SyncMessage::SyncStep2(update)))
                    | Ok(Some(SyncMessage::Update(update))) => {
                        // Client sent an update — apply to room and publish to NATS
                        match room.handle_client_update(client_id, &update).await {
                            Ok(raw_update) => {
                                if let Err(e) =
                                    nats_relay.publish_update(&document_id, &raw_update).await
                                {
                                    tracing::warn!(error = %e, "failed to publish update to NATS");
                                }
                            }
                            Err(e) => tracing::warn!(error = %e, "failed to handle client update"),
                        }
                    }
                    Ok(None) => {
                        tracing::debug!("received unknown message type, ignoring");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "invalid sync message");
                    }
                }
            }
            Message::Close(_) => break,
            _ => {} // Ignore text, ping, pong (axum handles pong automatically)
        }
    }

    // Cleanup
    send_task.abort();
    awareness_sub_cancel.cancel();

    // Remove from awareness and broadcast removal
    if let Some(removal_update) = awareness_manager.remove_client(awareness_client_id).await {
        broadcast_awareness_to_room(&room, client_id, &removal_update).await;
        if let Some(relay) = &awareness_relay {
            if let Err(e) = relay.publish(&document_id, &removal_update).await {
                tracing::warn!(error = %e, "failed to publish awareness removal to NATS");
            }
        }
    }

    let remaining = room.remove_client(client_id).await;

    tracing::info!(
        document_id = %document_id,
        user_id = %user_id,
        client_id = client_id,
        remaining_clients = remaining,
        "client disconnected"
    );

    // If no clients remain, destroy the room and awareness
    if remaining == 0 {
        nats_relay.unsubscribe(&document_id).await;
        room_manager.remove_room(&document_id).await;
        room_manager.remove_awareness(&document_id).await;
        tracing::info!(document_id = %document_id, "room destroyed");
    }
}

fn encode_awareness_message(data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + data.len());
    buf.push(MSG_AWARENESS);
    buf.extend_from_slice(data);
    buf
}

async fn broadcast_awareness_to_room(room: &DocumentRoom, exclude_client: u64, update: &[u8]) {
    let msg = encode_awareness_message(update);
    let clients = room.clients_read().await;
    for (&id, client) in clients.iter() {
        if id != exclude_client {
            let _ = client.sender.try_send(msg.clone());
        }
    }
}
