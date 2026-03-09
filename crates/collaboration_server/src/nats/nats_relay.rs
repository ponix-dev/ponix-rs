use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use common::nats::JetStreamPublisher;

use crate::domain::DocumentRoom;

/// Convert chrono DateTime<Utc> to time::OffsetDateTime for NATS API
fn chrono_to_time(dt: chrono::DateTime<chrono::Utc>) -> time::OffsetDateTime {
    time::OffsetDateTime::from_unix_timestamp(dt.timestamp())
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
}

pub struct NatsDocumentRelay {
    jetstream_publisher: Option<Arc<dyn JetStreamPublisher>>,
    jetstream_context: Option<async_nats::jetstream::Context>,
    instance_id: String,
    subscriptions: RwLock<HashMap<String, CancellationToken>>,
}

impl NatsDocumentRelay {
    pub fn new(
        jetstream_publisher: Arc<dyn JetStreamPublisher>,
        jetstream_context: async_nats::jetstream::Context,
    ) -> Self {
        Self {
            jetstream_publisher: Some(jetstream_publisher),
            jetstream_context: Some(jetstream_context),
            instance_id: uuid::Uuid::new_v4().to_string(),
            subscriptions: RwLock::new(HashMap::new()),
        }
    }

    /// Create a stub relay for testing (no NATS connection)
    #[cfg(any(test, feature = "integration-tests"))]
    pub fn new_stub() -> Self {
        Self {
            jetstream_publisher: None,
            jetstream_context: None,
            instance_id: "test-instance".to_string(),
            subscriptions: RwLock::new(HashMap::new()),
        }
    }

    /// Publish an update to JetStream for durability (snapshotter) and cross-instance relay
    pub async fn publish_update(
        &self,
        document_id: &str,
        update: &[u8],
    ) -> Result<(), anyhow::Error> {
        let publisher = self
            .jetstream_publisher
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no JetStream publisher configured"))?;

        let subject = format!("document_sync.{}", document_id);
        let mut headers = async_nats::HeaderMap::new();
        headers.insert("X-Instance-Id", self.instance_id.as_str());

        publisher
            .publish_with_headers(subject, headers, bytes::Bytes::from(update.to_vec()))
            .await?;

        Ok(())
    }

    /// Subscribe to JetStream for cross-instance updates on a document.
    /// Replays messages from `updated_at` for catchup, then continues delivering new messages.
    /// Spawns a background task that forwards received updates to the room.
    pub async fn subscribe(
        &self,
        document_id: &str,
        room: Arc<DocumentRoom>,
        updated_at: Option<chrono::DateTime<chrono::Utc>>,
        stream_name: &str,
    ) -> Result<(), anyhow::Error> {
        let js_context = match &self.jetstream_context {
            Some(ctx) => ctx,
            None => return Ok(()), // Stub mode for testing
        };

        use futures_util::StreamExt;

        let deliver_policy = match updated_at {
            Some(ts) => async_nats::jetstream::consumer::DeliverPolicy::ByStartTime {
                start_time: chrono_to_time(ts),
            },
            None => async_nats::jetstream::consumer::DeliverPolicy::New,
        };

        let consumer = js_context
            .create_consumer_on_stream(
                async_nats::jetstream::consumer::pull::Config {
                    filter_subject: format!("document_sync.{}", document_id),
                    deliver_policy,
                    ..Default::default()
                },
                stream_name,
            )
            .await?;

        let mut messages = consumer.messages().await?;
        let instance_id = self.instance_id.clone();
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_clone.cancelled() => break,
                    msg = messages.next() => {
                        let Some(Ok(msg)) = msg else { break };
                        // Skip our own messages
                        if let Some(headers) = &msg.headers {
                            if let Some(val) = headers.get("X-Instance-Id") {
                                if val.as_str() == instance_id {
                                    let _ = msg.ack().await;
                                    continue;
                                }
                            }
                        }
                        if let Err(e) = room.handle_remote_update(&msg.payload).await {
                            tracing::warn!(error = %e, "failed to apply remote update");
                        }
                        if let Err(e) = msg.ack().await {
                            tracing::warn!(error = %e, "failed to ack message");
                        }
                    }
                }
            }
        });

        self.subscriptions
            .write()
            .await
            .insert(document_id.to_string(), cancel);
        Ok(())
    }

    /// Unsubscribe from a document's updates (cancels the relay task)
    pub async fn unsubscribe(&self, document_id: &str) {
        if let Some(cancel) = self.subscriptions.write().await.remove(document_id) {
            cancel.cancel();
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}
