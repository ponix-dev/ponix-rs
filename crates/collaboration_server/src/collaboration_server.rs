use std::sync::Arc;

use common::auth::AuthTokenProvider;
use common::domain::{DocumentRepository, UserRepository};
use ponix_runner::AppProcess;

use crate::domain::RoomManager;
use crate::nats::{AwarenessRelay, NatsDocumentRelay};
use crate::websocket::{build_router, AppState};

pub struct CollaborationServerConfig {
    pub host: String,
    pub port: u16,
    pub cors_allowed_origins: String,
    pub document_updates_stream: String,
}

pub struct CollaborationServer {
    config: CollaborationServerConfig,
    app_state: Arc<AppState>,
}

impl CollaborationServer {
    pub fn new(
        config: CollaborationServerConfig,
        document_repository: Arc<dyn DocumentRepository>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
        user_repository: Arc<dyn UserRepository>,
        nats_relay: Arc<NatsDocumentRelay>,
        awareness_relay: Option<Arc<AwarenessRelay>>,
    ) -> Self {
        let room_manager = Arc::new(RoomManager::new(
            document_repository,
            nats_relay.clone(),
            config.document_updates_stream.clone(),
        ));

        let app_state = Arc::new(AppState {
            room_manager,
            nats_relay,
            awareness_relay,
            auth_token_provider,
            user_repository,
        });

        Self { config, app_state }
    }

    pub fn into_runner_process(self) -> AppProcess {
        Box::new(move |cancellation_token| {
            Box::pin(async move {
                let addr = format!("{}:{}", self.config.host, self.config.port);
                let router = build_router(self.app_state, &self.config.cors_allowed_origins);

                let listener = tokio::net::TcpListener::bind(&addr).await?;
                tracing::info!(address = %addr, "collaboration server started");

                axum::serve(listener, router)
                    .with_graceful_shutdown(async move {
                        cancellation_token.cancelled().await;
                        tracing::debug!("collaboration server shutdown signal received");
                    })
                    .await?;

                tracing::debug!("collaboration server stopped");
                Ok(())
            })
        })
    }
}
