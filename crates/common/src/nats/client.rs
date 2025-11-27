use crate::nats::trace_context::inject_trace_context;
use crate::nats::traits::{JetStreamConsumer, JetStreamPublisher, PullConsumer};
use anyhow::{Context, Result};
use async_nats::jetstream::{self, stream::Config as StreamConfig};
use async_nats::HeaderMap;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{error, info, instrument};

pub struct NatsClient {
    client: async_nats::Client,
    jetstream: jetstream::Context,
}

impl NatsClient {
    pub async fn connect(url: &str, timeout: std::time::Duration) -> Result<Self> {
        info!(url = %url, timeout_ms = timeout.as_millis(), "Connecting to NATS");

        // Configure connection timeout for establishing the TCP connection
        let client = async_nats::ConnectOptions::new()
            .connection_timeout(timeout)
            .connect(url)
            .await
            .context("Failed to connect to NATS")?;

        let jetstream = jetstream::new(client.clone());

        info!("Successfully connected to NATS");
        Ok(Self { client, jetstream })
    }

    pub async fn ensure_stream(&self, stream_name: &str) -> Result<()> {
        info!(stream = %stream_name, "Ensuring stream exists");

        let stream_config = StreamConfig {
            name: stream_name.to_string(),
            subjects: vec![format!("{}.*", stream_name)],
            description: Some("Stream for processed envelopes".to_string()),
            ..Default::default()
        };

        match self.jetstream.get_stream(stream_name).await {
            Ok(_) => {
                info!(stream = %stream_name, "Stream already exists");
            }
            Err(_) => {
                self.jetstream
                    .create_stream(stream_config)
                    .await
                    .context("Failed to create stream")?;
                info!(stream = %stream_name, "Created stream");
            }
        }

        Ok(())
    }

    pub fn jetstream(&self) -> &jetstream::Context {
        &self.jetstream
    }

    /// Create a JetStreamConsumer trait object from this client
    pub fn create_consumer_client(&self) -> Arc<dyn JetStreamConsumer> {
        Arc::new(NatsJetStreamConsumer::new(self.jetstream.clone()))
    }

    /// Create a JetStreamPublisher trait object from this client
    pub fn create_publisher_client(&self) -> Arc<dyn JetStreamPublisher> {
        Arc::new(NatsJetStreamPublisher::new(self.jetstream.clone()))
    }

    pub async fn close(self) {
        info!("Closing NATS connection");
        // Connection closes automatically when dropped
    }
}

#[allow(dead_code)]
impl NatsClient {
    // Keep client field for potential future use
    fn _client(&self) -> &async_nats::Client {
        &self.client
    }
}

/// Concrete implementation of JetStreamConsumer using async-nats
pub struct NatsJetStreamConsumer {
    context: jetstream::Context,
}

impl NatsJetStreamConsumer {
    pub fn new(context: jetstream::Context) -> Self {
        Self { context }
    }
}

#[async_trait]
impl JetStreamConsumer for NatsJetStreamConsumer {
    async fn create_consumer(
        &self,
        config: jetstream::consumer::pull::Config,
        stream_name: &str,
    ) -> Result<Box<dyn PullConsumer>> {
        let consumer = self
            .context
            .create_consumer_on_stream(config, stream_name)
            .await
            .context("Failed to create consumer")?;

        Ok(Box::new(NatsPullConsumer { consumer }))
    }
}

/// Concrete implementation of PullConsumer using async-nats
pub struct NatsPullConsumer {
    consumer: jetstream::consumer::PullConsumer,
}

#[async_trait]
impl PullConsumer for NatsPullConsumer {
    async fn fetch_messages(
        &self,
        max_messages: usize,
        expires: std::time::Duration,
    ) -> Result<Vec<jetstream::Message>> {
        use futures::StreamExt;

        let mut messages = self
            .consumer
            .fetch()
            .max_messages(max_messages)
            .expires(expires)
            .messages()
            .await
            .context("Failed to fetch messages")?;

        let mut result = Vec::new();
        while let Some(msg) = messages.next().await {
            match msg {
                Ok(message) => result.push(message),
                Err(e) => {
                    error!(error = %e, "Error receiving message");
                    // Continue processing other messages
                }
            }
        }
        Ok(result)
    }
}

/// Concrete implementation of JetStreamPublisher using async-nats
pub struct NatsJetStreamPublisher {
    context: jetstream::Context,
}

impl NatsJetStreamPublisher {
    pub fn new(context: jetstream::Context) -> Self {
        Self { context }
    }
}

#[async_trait]
impl JetStreamPublisher for NatsJetStreamPublisher {
    async fn get_stream(&self, stream_name: &str) -> Result<()> {
        self.context
            .get_stream(stream_name)
            .await
            .context("Failed to get stream")?;
        Ok(())
    }

    async fn create_stream(&self, config: jetstream::stream::Config) -> Result<()> {
        self.context
            .create_stream(config)
            .await
            .context("Failed to create stream")?;
        Ok(())
    }

    #[instrument(skip(self, payload), fields(subject = %subject, payload_size = payload.len()))]
    async fn publish(&self, subject: String, payload: bytes::Bytes) -> Result<()> {
        // Inject trace context into headers for distributed tracing
        let mut headers = HeaderMap::new();
        inject_trace_context(&mut headers);

        let ack = self
            .context
            .publish_with_headers(subject, headers, payload)
            .await
            .context("Failed to publish message to JetStream")?;

        ack.await
            .context("Failed to receive JetStream acknowledgment")?;
        Ok(())
    }
}
