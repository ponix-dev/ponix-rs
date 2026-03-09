use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use common::auth::AuthTokenProvider;
use futures_util::StreamExt;

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Wait for the first WebSocket message containing a JWT, validate it,
/// and return the user_id. Closes the socket on failure.
///
/// Protocol: client sends a text message containing the raw JWT string
/// as the first message after WebSocket upgrade. Server validates and
/// either proceeds or closes with a reason.
pub async fn authenticate_first_message(
    socket: &mut WebSocket,
    auth_provider: &dyn AuthTokenProvider,
) -> Result<String, AuthError> {
    let msg = tokio::time::timeout(AUTH_TIMEOUT, socket.next())
        .await
        .map_err(|_| AuthError::Timeout)?
        .ok_or(AuthError::ConnectionClosed)?
        .map_err(|_| AuthError::ConnectionClosed)?;

    match msg {
        Message::Text(token) => auth_provider
            .validate_token(token.trim())
            .map_err(|_| AuthError::InvalidToken),
        _ => Err(AuthError::InvalidMessage),
    }
}

#[derive(Debug)]
pub enum AuthError {
    InvalidToken,
    InvalidMessage,
    Timeout,
    ConnectionClosed,
}

impl AuthError {
    pub fn close_reason(&self) -> &'static str {
        match self {
            AuthError::InvalidToken => "invalid or expired token",
            AuthError::InvalidMessage => "expected text message with JWT",
            AuthError::Timeout => "auth timeout exceeded",
            AuthError::ConnectionClosed => "connection closed before auth",
        }
    }
}
