use anyhow::{Context, Result};
use async_nats::jetstream::{self, stream::Config as StreamConfig};
use tracing::info;

pub struct NatsClient {
    client: async_nats::Client,
    jetstream: jetstream::Context,
}

impl NatsClient {
    pub async fn connect(url: &str, timeout: std::time::Duration) -> Result<Self> {
        info!("Connecting to NATS at {} (timeout={:?})", url, timeout);

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
        info!("Ensuring stream '{}' exists", stream_name);

        let stream_config = StreamConfig {
            name: stream_name.to_string(),
            subjects: vec![format!("{}.*", stream_name)],
            description: Some("Stream for processed envelopes".to_string()),
            ..Default::default()
        };

        match self.jetstream.get_stream(stream_name).await {
            Ok(_) => {
                info!("Stream '{}' already exists", stream_name);
            }
            Err(_) => {
                self.jetstream
                    .create_stream(stream_config)
                    .await
                    .context("Failed to create stream")?;
                info!("Created stream '{}'", stream_name);
            }
        }

        Ok(())
    }

    pub fn jetstream(&self) -> &jetstream::Context {
        &self.jetstream
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
