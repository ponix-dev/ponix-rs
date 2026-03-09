use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use common::auth::AuthTokenProvider;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::domain::{decode_sync_message, encode_sync_step1, encode_sync_step2, SyncMessage};
use crate::domain::{DocumentRoom, RoomManager};
use crate::nats::NatsDocumentRelay;
use crate::websocket::auth::authenticate_first_message;

pub async fn handle_connection(
    mut socket: WebSocket,
    room: Arc<DocumentRoom>,
    room_manager: Arc<RoomManager>,
    nats_relay: Arc<NatsDocumentRelay>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
) {
    let document_id = room.document_id().to_string();

    // 1. First-message auth: wait for JWT as first text message
    let user_id = match authenticate_first_message(&mut socket, auth_token_provider.as_ref()).await
    {
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

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // 2. Create channel for outbound messages (room broadcasts to us via this)
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(256);

    // 3. Add client to room — get initial sync data
    let (client_id, state_vector, full_update) = room.add_client(user_id.clone(), tx).await;

    tracing::info!(
        document_id = %document_id,
        user_id = %user_id,
        client_id = client_id,
        "client authenticated and connected"
    );

    // 4. Send initial sync: SyncStep1 (our state vector) + SyncStep2 (full state)
    let step1 = encode_sync_step1(&state_vector);
    let step2 = encode_sync_step2(&full_update);
    if ws_sender.send(Message::Binary(step1.into())).await.is_err()
        || ws_sender.send(Message::Binary(step2.into())).await.is_err()
    {
        room.remove_client(client_id).await;
        return;
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
                        // Non-sync message (e.g., awareness — #178 will handle)
                        tracing::debug!("received non-sync message, ignoring");
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
    let remaining = room.remove_client(client_id).await;

    tracing::info!(
        document_id = %document_id,
        user_id = %user_id,
        client_id = client_id,
        remaining_clients = remaining,
        "client disconnected"
    );

    // If no clients remain, destroy the room
    if remaining == 0 {
        nats_relay.unsubscribe(&document_id).await;
        room_manager.remove_room(&document_id).await;
        tracing::info!(document_id = %document_id, "room destroyed");
    }
}
