use crate::domain::result::DomainResult;
use async_trait::async_trait;

/// Domain entity for a processed envelope
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedEnvelope {
    pub organization_id: String,
    pub end_device_id: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub processed_at: chrono::DateTime<chrono::Utc>,
    pub data: serde_json::Map<String, serde_json::Value>,
}

/// Input for storing processed envelopes (batch operation)
#[derive(Debug, Clone)]
pub struct StoreEnvelopesInput {
    pub envelopes: Vec<ProcessedEnvelope>,
}

/// Raw envelope with binary payload before processing
#[derive(Debug, Clone, PartialEq)]
pub struct RawEnvelope {
    pub organization_id: String,
    pub end_device_id: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub payload: Vec<u8>,
}

/// Trait for publishing processed envelopes to message broker
///
/// Implementations should:
/// - Serialize envelope to appropriate format (protobuf)
/// - Publish to message broker (NATS JetStream)
/// - Return error if publish fails
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait ProcessedEnvelopeProducer: Send + Sync {
    /// Publish a single processed envelope
    ///
    /// # Arguments
    /// * `envelope` - ProcessedEnvelope to publish
    ///
    /// # Returns
    /// () on success, DomainError on failure
    async fn publish(&self, envelope: &ProcessedEnvelope) -> DomainResult<()>;
}

/// Trait for publishing raw envelopes to message broker
///
/// Implementations should:
/// - Serialize envelope to appropriate format (protobuf)
/// - Publish to message broker (NATS JetStream)
/// - Return error if publish fails
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait RawEnvelopeProducer: Send + Sync {
    /// Publish a single raw envelope
    ///
    /// # Arguments
    /// * `envelope` - RawEnvelope to publish
    ///
    /// # Returns
    /// () on success, DomainError on failure
    async fn publish(&self, envelope: &RawEnvelope) -> DomainResult<()>;
}

/// Repository trait for processed envelope storage operations
/// Infrastructure layer (e.g., ponix-clickhouse) implements this trait
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait ProcessedEnvelopeRepository: Send + Sync {
    /// Store a batch of processed envelopes
    /// Implementations should handle chunking if needed for large batches
    /// Failure handling: entire batch fails atomically (all-or-nothing)
    async fn store_batch(&self, input: StoreEnvelopesInput) -> DomainResult<()>;
}
