use std::sync::Arc;

use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Response;
use common::auth::AuthTokenProvider;

use crate::nats::NatsDocumentRelay;
use crate::domain::RoomManager;
use crate::websocket::connection::handle_connection;

pub struct AppState {
    pub room_manager: Arc<RoomManager>,
    pub nats_relay: Arc<NatsDocumentRelay>,
    pub auth_token_provider: Arc<dyn AuthTokenProvider>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(document_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, StatusCode> {
    // Check document exists before upgrading (avoids holding unauthenticated
    // WebSocket connections for non-existent documents)
    let room = state
        .room_manager
        .get_or_create_room(&document_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to get or create room");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(ws.on_upgrade(move |socket| {
        handle_connection(
            socket,
            room,
            state.room_manager.clone(),
            state.nats_relay.clone(),
            state.auth_token_provider.clone(),
        )
    }))
}

/// Build the Axum router
pub fn build_router(state: Arc<AppState>, cors_allowed_origins: &str) -> axum::Router {
    let cors = build_cors_layer(cors_allowed_origins);

    axum::Router::new()
        .route(
            "/ws/documents/{document_id}",
            axum::routing::get(ws_handler),
        )
        .layer(cors)
        .with_state(state)
}

fn build_cors_layer(origins: &str) -> tower_http::cors::CorsLayer {
    if origins == "*" {
        tower_http::cors::CorsLayer::permissive()
    } else {
        use axum::http::HeaderValue;
        let allowed: Vec<HeaderValue> = origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        tower_http::cors::CorsLayer::new().allow_origin(allowed)
    }
}
