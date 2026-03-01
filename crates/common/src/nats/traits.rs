use anyhow::Result;
use async_nats::jetstream;
use async_nats::HeaderMap;
use async_trait::async_trait;

pub use async_nats::jetstream::object_store::ObjectInfo;

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
    /// Note: This method does NOT inject trace context - use the middleware stack instead
    async fn publish(&self, subject: String, payload: bytes::Bytes) -> Result<()>;

    /// Publish a message with custom headers and await acknowledgment
    /// Used by the middleware stack to publish with trace context headers
    async fn publish_with_headers(
        &self,
        subject: String,
        headers: HeaderMap,
        payload: bytes::Bytes,
    ) -> Result<()>;
}

/// Trait for document/blob storage operations backed by NATS Object Store
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait DocumentContentStore: Send + Sync {
    /// Upload content with the given key
    async fn upload(&self, key: &str, content: bytes::Bytes) -> Result<()>;

    /// Download content by key
    async fn download(&self, key: &str) -> Result<bytes::Bytes>;

    /// Delete an object by key
    async fn delete(&self, key: &str) -> Result<()>;

    /// Get metadata about an object by key
    async fn info(&self, key: &str) -> Result<ObjectInfo>;

    /// Check if an object exists by key
    async fn exists(&self, key: &str) -> Result<bool>;
}
