use anyhow::Result;
use async_nats::jetstream;
use async_trait::async_trait;

/// Trait for JetStream consumer operations
/// Abstracts the operations needed to create and use a NATS JetStream consumer
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait JetStreamConsumer: Send + Sync {
    /// Create a durable pull consumer on a stream
    async fn create_consumer(
        &self,
        config: jetstream::consumer::pull::Config,
        stream_name: &str,
    ) -> Result<Box<dyn PullConsumer>>;
}

/// Trait for pull consumer operations
/// Abstracts the fetch operation on a pull consumer
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait PullConsumer: Send + Sync {
    /// Fetch messages from the consumer
    /// Returns a batch of messages up to max_messages, waiting up to expires duration
    async fn fetch_messages(
        &self,
        max_messages: usize,
        expires: std::time::Duration,
    ) -> Result<Vec<jetstream::Message>>;
}

/// Trait for JetStream publisher operations
/// Abstracts the operations needed to create streams and publish messages
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait JetStreamPublisher: Send + Sync {
    /// Get an existing stream by name
    async fn get_stream(&self, stream_name: &str) -> Result<()>;

    /// Create a new stream with the given configuration
    async fn create_stream(&self, config: jetstream::stream::Config) -> Result<()>;

    /// Publish a message to a subject and await acknowledgment
    async fn publish(&self, subject: String, payload: bytes::Bytes) -> Result<()>;
}
