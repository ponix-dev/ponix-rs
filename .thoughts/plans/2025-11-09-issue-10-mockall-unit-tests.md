# Add mockall support and unit tests for ponix-nats package

## Overview

Add mockall testing infrastructure to the ponix-nats crate to enable proper unit testing with mocked dependencies. Currently, all components directly depend on concrete async-nats types (specifically `jetstream::Context`), making unit testing impossible without a real NATS server. This plan introduces trait-based abstractions for JetStream operations, adds mockall as a dev dependency, and implements comprehensive unit tests for all ponix-nats components.

## Current State Analysis

### What Exists Now:
- **ponix-nats crate** with four main components:
  - `NatsClient` - manages NATS connections and stream provisioning ([client.rs:5-8](crates/ponix-nats/src/client.rs#L5-L8))
  - `NatsConsumer` - generic batch consumer ([consumer.rs:50-55](crates/ponix-nats/src/consumer.rs#L50-L55))
  - `ProcessedEnvelopeProducer` - publishes protobuf messages (feature-gated) ([processed_envelope_producer.rs:13-16](crates/ponix-nats/src/processed_envelope_producer.rs#L13-L16))
  - `ProcessedEnvelopeProcessor` - default processor factory (feature-gated) ([processed_envelope_processor.rs:18](crates/ponix-nats/src/processed_envelope_processor.rs#L18))

- **Direct dependencies on concrete types**:
  - All components use `async_nats::jetstream::Context` directly
  - No trait abstractions for external dependencies
  - No mocking infrastructure

- **Minimal testing in workspace**:
  - Only the `runner` crate has unit tests (2 tests)
  - No tests in ponix-nats crate
  - No mocking libraries used anywhere
  - No dev-dependencies for testing in ponix-nats

### Key Discoveries:
- JetStream context is used for three main operations: stream management, consumer creation, and message publishing
- `NatsConsumer` and `ProcessedEnvelopeProducer` are the primary components that need mocking
- The `async_nats::jetstream::Message` type is just a data container and doesn't need abstraction
- Mockall uses `#[automock]` attribute macros, no separate code generation step needed
- The workspace uses `#[tokio::test]` for async testing (established pattern in runner crate)

### What's Missing:
- Trait abstractions for JetStream operations
- mockall dependency
- Unit tests for all components
- Test utilities or helpers
- Documentation on testing patterns

## Desired End State

After this plan is complete, the ponix-nats crate will have:

1. **Two trait abstractions**:
   - `JetStreamConsumer` - for consumer operations (create consumer, fetch messages)
   - `JetStreamPublisher` - for publisher operations (create/verify streams, publish messages)

2. **Refactored components** accepting trait objects:
   - `NatsClient` parameterized with concrete JetStream context (provides trait implementations)
   - `NatsConsumer` accepting `Arc<dyn JetStreamConsumer>`
   - `ProcessedEnvelopeProducer` accepting `Arc<dyn JetStreamPublisher>`

3. **Comprehensive unit tests** covering:
   - All happy path scenarios
   - Error handling and edge cases
   - Feature-gated components
   - Fine-grained ack/nak behavior

4. **Testing infrastructure**:
   - mockall as a dev dependency
   - Mock implementations generated via `#[automock]`
   - Test utilities for common setups

### Verification:
- All unit tests pass: `cargo test -p ponix-nats`
- Tests can run without a NATS server
- Feature-gated tests work: `cargo test -p ponix-nats --features processed-envelope`
- No regressions in ponix-all-in-one integration
- Code compiles and lints cleanly: `cargo build && cargo clippy`

## What We're NOT Doing

- NOT adding integration tests with a real NATS server (that's a separate concern)
- NOT changing the public API surface (trait implementations are internal)
- NOT refactoring the BatchProcessor pattern (it's already testable)
- NOT mocking the `Message` type (it's just a data container)
- NOT adding mockall to other crates in the workspace (focused on ponix-nats only)
- NOT adding a code generation step or mise task (mockall handles it with attributes)
- NOT testing the ponix-all-in-one integration (that requires integration tests)

## Implementation Approach

We'll implement this in phases to maintain testability at each step:

1. **Phase 1**: Add mockall dependency and create trait abstractions
2. **Phase 2**: Refactor NatsClient to work with traits
3. **Phase 3**: Refactor NatsConsumer and add unit tests
4. **Phase 4**: Refactor ProcessedEnvelopeProducer and add unit tests
5. **Phase 5**: Test ProcessedEnvelopeProcessor and update ponix-all-in-one
6. **Phase 6**: Documentation and cleanup

Each phase will be independently testable before moving to the next.

---

## Phase 1: Add mockall and Create Trait Abstractions

### Overview
Add mockall as a dev dependency and create two trait abstractions: `JetStreamConsumer` for consumer operations and `JetStreamPublisher` for publisher operations. These traits will define the interface we need to mock for testing.

### Changes Required:

#### 1. Update Cargo.toml
**File**: `crates/ponix-nats/Cargo.toml`
**Changes**: Add mockall as a dev dependency

```toml
[dev-dependencies]
mockall = "0.13"
```

#### 2. Create traits module
**File**: `crates/ponix-nats/src/traits.rs` (NEW FILE)
**Changes**: Create trait abstractions for JetStream operations

```rust
use async_nats::jetstream;
use async_trait::async_trait;
use anyhow::Result;

/// Trait for JetStream consumer operations
/// Abstracts the operations needed to create and use a NATS JetStream consumer
#[cfg_attr(test, mockall::automock)]
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
#[cfg_attr(test, mockall::automock)]
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
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait JetStreamPublisher: Send + Sync {
    /// Get an existing stream by name
    async fn get_stream(&self, stream_name: &str) -> Result<()>;

    /// Create a new stream with the given configuration
    async fn create_stream(&self, config: jetstream::stream::Config) -> Result<()>;

    /// Publish a message to a subject and await acknowledgment
    async fn publish(&self, subject: String, payload: bytes::Bytes) -> Result<()>;
}
```

#### 3. Update lib.rs to export traits
**File**: `crates/ponix-nats/src/lib.rs`
**Changes**: Add module declaration and export traits

```rust
// Add after line 2
mod traits;

// Add after line 10 (or in appropriate location)
pub use traits::{JetStreamConsumer, JetStreamPublisher, PullConsumer};
```

#### 4. Add async-trait dependency
**File**: `crates/ponix-nats/Cargo.toml`
**Changes**: Add async-trait for trait async methods

```toml
# Add to [dependencies] section
async-trait = "0.1"
bytes = "1.0"  # Already used by async-nats, make it explicit
```

### Success Criteria:

#### Automated Verification:
- [x] Cargo.toml includes mockall dev-dependency: `grep -q "mockall" crates/ponix-nats/Cargo.toml`
- [x] Traits module compiles: `cargo build -p ponix-nats`
- [x] Mock types are generated in test mode: `cargo test -p ponix-nats --no-run`
- [x] No linting errors: `cargo clippy -p ponix-nats`
- [x] Traits are exported in public API: `cargo doc -p ponix-nats --no-deps`

#### Manual Verification:
- [ ] Traits module structure is clean and well-documented
- [ ] Mock attribute is correctly applied (only in test mode)
- [ ] Trait methods match the actual usage patterns identified in research

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation from the human that the manual testing was successful before proceeding to the next phase.

---

## Phase 2: Refactor NatsClient with Trait Implementations

### Overview
Refactor `NatsClient` to provide concrete implementations of the JetStream traits. This allows the client to act as an adapter between async-nats and our trait abstractions, while keeping the existing public API intact.

### Changes Required:

#### 1. Create JetStream adapter structs
**File**: `crates/ponix-nats/src/client.rs`
**Changes**: Add adapter structs that implement the traits

```rust
// Add these imports at the top
use crate::traits::{JetStreamConsumer, JetStreamPublisher, PullConsumer};
use async_trait::async_trait;
use std::sync::Arc;

// Add after the NatsClient impl block (after line 60)

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
                    error!("Error receiving message: {}", e);
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

    async fn publish(&self, subject: String, payload: bytes::Bytes) -> Result<()> {
        let ack = self
            .context
            .publish(subject, payload)
            .await
            .context("Failed to publish message to JetStream")?;

        ack.await
            .context("Failed to receive JetStream acknowledgment")?;
        Ok(())
    }
}
```

#### 2. Add helper methods to NatsClient
**File**: `crates/ponix-nats/src/client.rs`
**Changes**: Add methods to create trait implementations

```rust
// Add these methods to the NatsClient impl block (before the closing brace)

    /// Create a JetStreamConsumer trait object from this client
    pub fn create_consumer_client(&self) -> Arc<dyn JetStreamConsumer> {
        Arc::new(NatsJetStreamConsumer::new(self.jetstream.clone()))
    }

    /// Create a JetStreamPublisher trait object from this client
    pub fn create_publisher_client(&self) -> Arc<dyn JetStreamPublisher> {
        Arc::new(NatsJetStreamPublisher::new(self.jetstream.clone()))
    }
```

#### 3. Update lib.rs exports
**File**: `crates/ponix-nats/src/lib.rs`
**Changes**: Export the new adapter types

```rust
// Update the client export line to include new types
pub use client::{NatsClient, NatsJetStreamConsumer, NatsJetStreamPublisher, NatsPullConsumer};
```

#### 4. Add bytes to dependencies
**File**: `crates/ponix-nats/Cargo.toml`
**Changes**: Already added in Phase 1

### Success Criteria:

#### Automated Verification:
- [x] Code compiles: `cargo build -p ponix-nats`
- [x] All types are exported correctly: `cargo doc -p ponix-nats --no-deps`
- [x] No clippy warnings: `cargo clippy -p ponix-nats`
- [x] Existing integration still works: `cargo build -p ponix-all-in-one`

#### Manual Verification:
- [ ] Adapter implementations correctly delegate to async-nats
- [ ] Error handling is preserved from original implementation
- [ ] Trait objects can be created from NatsClient
- [ ] Public API remains unchanged for existing consumers

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation from the human that the manual testing was successful before proceeding to the next phase.

---

## Phase 3: Refactor NatsConsumer and Add Unit Tests

### Overview
Refactor `NatsConsumer` to accept `Arc<dyn JetStreamConsumer>` instead of directly using `jetstream::Context`, then implement comprehensive unit tests covering all scenarios including happy paths, error cases, and fine-grained ack/nak behavior.

### Changes Required:

#### 1. Refactor NatsConsumer struct
**File**: `crates/ponix-nats/src/consumer.rs`
**Changes**: Update to use trait instead of concrete consumer

```rust
// Update imports (add to existing use statements)
use crate::traits::{JetStreamConsumer, PullConsumer};
use std::sync::Arc;

// Update the NatsConsumer struct (lines 50-55)
pub struct NatsConsumer {
    consumer: Box<dyn PullConsumer>,
    batch_size: usize,
    max_wait: Duration,
    processor: BatchProcessor,
}
```

#### 2. Update NatsConsumer::new() constructor
**File**: `crates/ponix-nats/src/consumer.rs`
**Changes**: Accept trait object and use it to create consumer

```rust
// Replace the entire new() method (lines 58-101)
impl NatsConsumer {
    pub async fn new(
        jetstream: Arc<dyn JetStreamConsumer>,
        stream_name: &str,
        consumer_name: &str,
        subject_filter: &str,
        batch_size: usize,
        max_wait_secs: u64,
        processor: BatchProcessor,
    ) -> Result<Self> {
        let config = jetstream::consumer::pull::Config {
            name: Some(consumer_name.to_string()),
            durable_name: Some(consumer_name.to_string()),
            filter_subject: subject_filter.to_string(),
            ack_policy: jetstream::consumer::AckPolicy::Explicit,
            ..Default::default()
        };

        let consumer = jetstream
            .create_consumer(config, stream_name)
            .await
            .context("Failed to create consumer")?;

        Ok(Self {
            consumer,
            batch_size,
            max_wait: Duration::from_secs(max_wait_secs),
            processor,
        })
    }
}
```

#### 3. Update fetch_and_process_batch() method
**File**: `crates/ponix-nats/src/consumer.rs`
**Changes**: Use the trait's fetch_messages method

```rust
// Update fetch_and_process_batch (lines 126-239)
// Replace the message fetching section (lines 134-155)

    async fn fetch_and_process_batch(&self) -> Result<()> {
        debug!(
            "Fetching batch of up to {} messages with timeout {:?}",
            self.batch_size, self.max_wait
        );

        // Fetch messages using the trait method
        let messages = self
            .consumer
            .fetch_messages(self.batch_size, self.max_wait)
            .await?;

        if messages.is_empty() {
            debug!("No messages received in this batch");
            return Ok(());
        }

        info!("Received {} messages", messages.len());

        // Rest of the method remains the same (processing, ack, nak logic)
        // ... (lines 166-239 stay unchanged)
    }
```

#### 4. Add comprehensive unit tests
**File**: `crates/ponix-nats/src/consumer.rs`
**Changes**: Add test module at the end of the file

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{MockJetStreamConsumer, MockPullConsumer};
    use async_nats::jetstream;
    use bytes::Bytes;
    use mockall::predicate::*;
    use std::sync::Arc;
    use tokio::time::Duration;

    // Helper to create a test message
    fn create_test_message(payload: &str, subject: &str) -> jetstream::Message {
        // Note: This is a simplified version. In reality, you may need to use
        // the actual async-nats message creation or use a test fixture.
        // For now, we'll create a placeholder that compiles.
        // You may need to adjust based on async-nats Message construction.
        todo!("Create test message - implementation depends on async-nats version")
    }

    #[tokio::test]
    async fn test_consumer_creation_success() {
        let mut mock_jetstream = MockJetStreamConsumer::new();
        let mut mock_consumer = MockPullConsumer::new();

        // Set up expectations
        mock_jetstream
            .expect_create_consumer()
            .with(
                function(|config: &jetstream::consumer::pull::Config| {
                    config.durable_name.as_ref().unwrap() == "test-consumer"
                }),
                eq("test-stream"),
            )
            .times(1)
            .returning(|_, _| Ok(Box::new(mock_consumer)));

        let processor: BatchProcessor = Box::new(|_msgs| {
            Box::pin(async { Ok(ProcessingResult::ack_all(0)) })
        });

        let result = NatsConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            processor,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_consumer_creation_failure() {
        let mut mock_jetstream = MockJetStreamConsumer::new();

        mock_jetstream
            .expect_create_consumer()
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("Failed to create consumer")));

        let processor: BatchProcessor = Box::new(|_msgs| {
            Box::pin(async { Ok(ProcessingResult::ack_all(0)) })
        });

        let result = NatsConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            processor,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to create consumer"));
    }

    #[tokio::test]
    async fn test_fetch_and_process_empty_batch() {
        let mut mock_jetstream = MockJetStreamConsumer::new();
        let mut mock_consumer = MockPullConsumer::new();

        // Consumer returns empty batch
        mock_consumer
            .expect_fetch_messages()
            .times(1)
            .returning(|_, _| Ok(vec![]));

        mock_jetstream
            .expect_create_consumer()
            .times(1)
            .returning(move |_, _| {
                let mut mock = MockPullConsumer::new();
                mock.expect_fetch_messages()
                    .times(1)
                    .returning(|_, _| Ok(vec![]));
                Ok(Box::new(mock))
            });

        let processor: BatchProcessor = Box::new(|_msgs| {
            Box::pin(async { Ok(ProcessingResult::ack_all(0)) })
        });

        let consumer = NatsConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            processor,
        )
        .await
        .unwrap();

        let result = consumer.fetch_and_process_batch().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_and_process_ack_all() {
        // Test that all messages are acknowledged when processor returns ack_all
        // Implementation needed based on Message mock strategy
        todo!("Implement after determining Message mocking approach")
    }

    #[tokio::test]
    async fn test_fetch_and_process_nak_all() {
        // Test that all messages are nak'd when processor returns nak_all
        todo!("Implement after determining Message mocking approach")
    }

    #[tokio::test]
    async fn test_fetch_and_process_partial_ack_nak() {
        // Test fine-grained ack/nak behavior with ProcessingResult::new()
        todo!("Implement after determining Message mocking approach")
    }

    #[tokio::test]
    async fn test_processor_error_handling() {
        // Test that processor errors result in all messages being nak'd
        todo!("Implement after determining Message mocking approach")
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        // Test that consumer stops when cancellation token is triggered
        todo!("Implement using tokio::time::timeout and cancellation")
    }
}
```

### Success Criteria:

#### Automated Verification:
- [x] Code compiles: `cargo build -p ponix-nats`
- [x] Basic tests pass: `cargo test -p ponix-nats consumer::tests::test_consumer_creation`
- [x] No clippy warnings: `cargo clippy -p ponix-nats`

#### Manual Verification:
- [ ] Review test coverage - are all important scenarios covered?
- [ ] Determine strategy for mocking Message type (it may need special handling)
- [ ] Complete the TODO test implementations
- [ ] Verify that error messages are helpful and accurate

**Implementation Note**: This phase has some TODOs for Message mocking that need to be resolved. After completing the basic structure and all automated verification passes, we need to determine the best strategy for mocking or creating test Messages before implementing the remaining tests. Pause here for manual confirmation before proceeding to the next phase.

---

## Phase 4: Refactor ProcessedEnvelopeProducer and Add Unit Tests

### Overview
Refactor `ProcessedEnvelopeProducer` to accept `Arc<dyn JetStreamPublisher>` instead of directly using `jetstream::Context`, then implement comprehensive unit tests covering publishing success, failures, and error cases.

### Changes Required:

#### 1. Refactor ProcessedEnvelopeProducer struct
**File**: `crates/ponix-nats/src/processed_envelope_producer.rs`
**Changes**: Update to use trait instead of concrete context

```rust
// Add to imports at the top
use crate::traits::JetStreamPublisher;
use std::sync::Arc;

// Update the struct (lines 13-16)
pub struct ProcessedEnvelopeProducer {
    jetstream: Arc<dyn JetStreamPublisher>,
    base_subject: String,
}
```

#### 2. Update ProcessedEnvelopeProducer::new() constructor
**File**: `crates/ponix-nats/src/processed_envelope_producer.rs`
**Changes**: Accept trait object instead of concrete context

```rust
// Update new() method (lines 20-29)
impl ProcessedEnvelopeProducer {
    pub fn new(jetstream: Arc<dyn JetStreamPublisher>, base_subject: String) -> Self {
        Self {
            jetstream,
            base_subject,
        }
    }
}
```

#### 3. Update publish() method
**File**: `crates/ponix-nats/src/processed_envelope_producer.rs`
**Changes**: Use trait method for publishing

```rust
// Update publish method (lines 31-63)
    pub async fn publish(&self, envelope: &ProcessedEnvelope) -> Result<()> {
        let payload = envelope.encode_to_vec();

        let subject = format!("{}.{}", self.base_subject, envelope.device_id);

        debug!(
            subject = %subject,
            envelope_id = %envelope.id,
            "Publishing ProcessedEnvelope to NATS"
        );

        // Use the trait method
        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .context("Failed to publish and acknowledge message")?;

        info!(
            subject = %subject,
            envelope_id = %envelope.id,
            "Successfully published ProcessedEnvelope"
        );

        Ok(())
    }
```

#### 4. Add comprehensive unit tests
**File**: `crates/ponix-nats/src/processed_envelope_producer.rs`
**Changes**: Add test module at the end of the file

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::MockJetStreamPublisher;
    use mockall::predicate::*;
    use std::sync::Arc;

    // Helper to create a test ProcessedEnvelope
    fn create_test_envelope(device_id: &str, envelope_id: &str) -> ProcessedEnvelope {
        ProcessedEnvelope {
            id: envelope_id.to_string(),
            device_id: device_id.to_string(),
            timestamp: Some(prost_types::Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_producer_creation() {
        let mock_jetstream = MockJetStreamPublisher::new();
        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        assert_eq!(producer.base_subject, "processed.envelopes");
    }

    #[tokio::test]
    async fn test_publish_success() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Set up expectation for successful publish
        mock_jetstream
            .expect_publish()
            .withf(|subject: &String, payload: &bytes::Bytes| {
                subject.starts_with("processed.envelopes.") && !payload.is_empty()
            })
            .times(1)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope = create_test_envelope("device-123", "envelope-456");
        let result = producer.publish(&envelope).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_with_correct_subject() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Verify the subject format is correct
        mock_jetstream
            .expect_publish()
            .with(
                eq("processed.envelopes.device-123".to_string()),
                always(),
            )
            .times(1)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope = create_test_envelope("device-123", "envelope-456");
        let result = producer.publish(&envelope).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_serializes_correctly() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Capture the payload to verify serialization
        mock_jetstream
            .expect_publish()
            .times(1)
            .withf(|_subject: &String, payload: &bytes::Bytes| {
                // Verify payload can be deserialized back
                ProcessedEnvelope::decode(payload.as_ref()).is_ok()
            })
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope = create_test_envelope("device-123", "envelope-456");
        let result = producer.publish(&envelope).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_failure() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Simulate publish failure
        mock_jetstream
            .expect_publish()
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("Failed to publish to JetStream")));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope = create_test_envelope("device-123", "envelope-456");
        let result = producer.publish(&envelope).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to publish"));
    }

    #[tokio::test]
    async fn test_publish_multiple_envelopes() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Should publish each envelope separately
        mock_jetstream
            .expect_publish()
            .times(3)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        for i in 0..3 {
            let envelope = create_test_envelope(
                &format!("device-{}", i),
                &format!("envelope-{}", i),
            );
            let result = producer.publish(&envelope).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_publish_different_device_ids() {
        let mut mock_jetstream = MockJetStreamPublisher::new();

        // Verify subjects change based on device_id
        mock_jetstream
            .expect_publish()
            .with(eq("processed.envelopes.device-A".to_string()), always())
            .times(1)
            .returning(|_, _| Ok(()));

        mock_jetstream
            .expect_publish()
            .with(eq("processed.envelopes.device-B".to_string()), always())
            .times(1)
            .returning(|_, _| Ok(()));

        let producer = ProcessedEnvelopeProducer::new(
            Arc::new(mock_jetstream),
            "processed.envelopes".to_string(),
        );

        let envelope_a = create_test_envelope("device-A", "envelope-1");
        let envelope_b = create_test_envelope("device-B", "envelope-2");

        assert!(producer.publish(&envelope_a).await.is_ok());
        assert!(producer.publish(&envelope_b).await.is_ok());
    }
}
```

### Success Criteria:

#### Automated Verification:
- [x] Code compiles with processed-envelope feature: `cargo build -p ponix-nats --features processed-envelope`
- [x] Tests pass: `cargo test -p ponix-nats --features processed-envelope processed_envelope_producer::tests`
- [x] No clippy warnings: `cargo clippy -p ponix-nats --features processed-envelope`
- [x] Tests compile without feature (trait still available): `cargo test -p ponix-nats --no-run`

#### Manual Verification:
- [ ] All test scenarios pass and provide good coverage
- [ ] Serialization/deserialization works correctly in tests
- [ ] Error messages are helpful
- [ ] Subject formatting matches expected pattern (base.device_id)

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation from the human that the manual testing was successful before proceeding to the next phase.

---

## Phase 5: Test ProcessedEnvelopeProcessor and Update ponix-all-in-one

### Overview
Add unit tests for the `create_processed_envelope_processor()` factory function, then update the ponix-all-in-one crate to use the new trait-based API.

### Changes Required:

#### 1. Add unit tests for ProcessedEnvelopeProcessor
**File**: `crates/ponix-nats/src/processed_envelope_processor.rs`
**Changes**: Add test module at the end of the file

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    // Helper to create a mock Message
    // Note: Implementation depends on how we solved Message mocking in Phase 3
    fn create_mock_message(payload: Vec<u8>) -> jetstream::Message {
        todo!("Use same approach as Phase 3")
    }

    #[tokio::test]
    async fn test_processor_success() {
        let processor = create_processed_envelope_processor();

        // Create a valid ProcessedEnvelope
        let envelope = ProcessedEnvelope {
            id: "test-id".to_string(),
            device_id: "device-123".to_string(),
            timestamp: Some(prost_types::Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
            ..Default::default()
        };

        let payload = envelope.encode_to_vec();
        let message = create_mock_message(payload);

        let result = processor(&[message]).await;
        assert!(result.is_ok());

        let processing_result = result.unwrap();
        assert_eq!(processing_result.ack.len(), 1);
        assert_eq!(processing_result.ack[0], 0);
        assert!(processing_result.nak.is_empty());
    }

    #[tokio::test]
    async fn test_processor_invalid_protobuf() {
        let processor = create_processed_envelope_processor();

        // Create invalid protobuf payload
        let payload = vec![0xFF, 0xFF, 0xFF];
        let message = create_mock_message(payload);

        let result = processor(&[message]).await;
        assert!(result.is_ok());

        let processing_result = result.unwrap();
        assert!(processing_result.ack.is_empty());
        assert_eq!(processing_result.nak.len(), 1);
        assert_eq!(processing_result.nak[0].0, 0);
        assert!(processing_result.nak[0].1.is_some());
    }

    #[tokio::test]
    async fn test_processor_multiple_messages() {
        let processor = create_processed_envelope_processor();

        // Create multiple valid envelopes
        let mut messages = vec![];
        for i in 0..5 {
            let envelope = ProcessedEnvelope {
                id: format!("test-id-{}", i),
                device_id: format!("device-{}", i),
                timestamp: Some(prost_types::Timestamp {
                    seconds: 1234567890 + i as i64,
                    nanos: 0,
                }),
                ..Default::default()
            };
            let payload = envelope.encode_to_vec();
            messages.push(create_mock_message(payload));
        }

        let result = processor(&messages).await;
        assert!(result.is_ok());

        let processing_result = result.unwrap();
        assert_eq!(processing_result.ack.len(), 5);
        assert!(processing_result.nak.is_empty());
    }

    #[tokio::test]
    async fn test_processor_mixed_valid_invalid() {
        let processor = create_processed_envelope_processor();

        let mut messages = vec![];

        // Valid message
        let envelope = ProcessedEnvelope {
            id: "valid".to_string(),
            device_id: "device-1".to_string(),
            ..Default::default()
        };
        messages.push(create_mock_message(envelope.encode_to_vec()));

        // Invalid message
        messages.push(create_mock_message(vec![0xFF, 0xFF]));

        // Another valid message
        let envelope2 = ProcessedEnvelope {
            id: "valid-2".to_string(),
            device_id: "device-2".to_string(),
            ..Default::default()
        };
        messages.push(create_mock_message(envelope2.encode_to_vec()));

        let result = processor(&messages).await;
        assert!(result.is_ok());

        let processing_result = result.unwrap();
        assert_eq!(processing_result.ack.len(), 2);
        assert!(processing_result.ack.contains(&0));
        assert!(processing_result.ack.contains(&2));
        assert_eq!(processing_result.nak.len(), 1);
        assert_eq!(processing_result.nak[0].0, 1);
    }

    #[tokio::test]
    async fn test_processor_empty_batch() {
        let processor = create_processed_envelope_processor();

        let result = processor(&[]).await;
        assert!(result.is_ok());

        let processing_result = result.unwrap();
        assert!(processing_result.ack.is_empty());
        assert!(processing_result.nak.is_empty());
    }
}
```

#### 2. Update ponix-all-in-one to use trait-based API
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Update to use the new trait-based constructors

```rust
// Update imports (around line 4-6)
use ponix_nats::{
    create_processed_envelope_processor, NatsClient, NatsConsumer, ProcessedEnvelopeProducer,
};

// Update the main function (around lines 44-86)
// Replace consumer creation section:

    // Create consumer using trait-based API
    let consumer_client = client.create_consumer_client();
    let processor = create_processed_envelope_processor();

    let consumer = NatsConsumer::new(
        consumer_client,
        &config.nats_stream_name,
        &config.nats_consumer_name,
        &format!("{}.*", config.nats_subject),
        config.nats_batch_size,
        config.nats_max_wait_seconds,
        processor,
    )
    .await
    .context("Failed to create NATS consumer")?;

// Replace producer creation section:

    // Create producer using trait-based API
    let publisher_client = client.create_publisher_client();
    let producer = ProcessedEnvelopeProducer::new(
        publisher_client,
        config.nats_subject.clone(),
    );
```

### Success Criteria:

#### Automated Verification:
- [x] Processor tests compile: `cargo test -p ponix-nats --features processed-envelope --no-run`
- [x] Processor tests pass: `cargo test -p ponix-nats --features processed-envelope processed_envelope_processor::tests`
- [x] ponix-all-in-one compiles: `cargo build -p ponix-all-in-one`
- [x] No clippy warnings: `cargo clippy -p ponix-all-in-one`
- [x] All workspace tests pass: `cargo test`

#### Manual Verification:
- [ ] ProcessedEnvelopeProcessor correctly handles valid protobuf messages
- [ ] Invalid protobuf messages are properly nak'd with error messages
- [ ] Mixed batches correctly separate acks and naks
- [ ] ponix-all-in-one integration is working (may need local NATS for full test)

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation from the human that the manual testing was successful before proceeding to the next phase.

---

## Phase 6: Complete Remaining Tests and Documentation

### Overview
Complete any remaining TODOs from Phase 3 (Message mocking strategy and remaining consumer tests), add documentation for the testing patterns, and ensure all tests pass with comprehensive coverage.

### Changes Required:

#### 1. Resolve Message mocking strategy
**Research and implement one of these approaches:**

**Option A: Use real Message instances (recommended)**
- Create helper function that constructs actual async-nats Messages using test data
- May require using async-nats test utilities or internal constructors
- Most realistic testing approach

**Option B: Create a wrapper trait for Message**
- Add `NatsMessage` trait with methods we use (payload, ack, ack_with)
- Wrap real Message in adapter, use mocks in tests
- More abstraction but complete control

**Option C: Test at a higher level**
- Test the public API with the trait mocks
- Don't test internal message handling (already covered by integration tests)
- Simpler but less thorough

**Decision needed**: Choose approach and implement consistently

#### 2. Complete NatsConsumer unit tests
**File**: `crates/ponix-nats/src/consumer.rs`
**Changes**: Implement the TODO tests from Phase 3

```rust
    #[tokio::test]
    async fn test_fetch_and_process_ack_all() {
        // Use chosen Message mocking strategy
        let mut mock_jetstream = MockJetStreamConsumer::new();
        let messages = vec![
            /* create test messages */
        ];

        // Set up mock to return messages
        mock_jetstream
            .expect_create_consumer()
            .returning(move |_, _| {
                let mut mock_consumer = MockPullConsumer::new();
                mock_consumer
                    .expect_fetch_messages()
                    .returning(move |_, _| Ok(messages.clone()));
                Ok(Box::new(mock_consumer))
            });

        // Processor that acks all
        let processor: BatchProcessor = Box::new(|msgs| {
            let count = msgs.len();
            Box::pin(async move { Ok(ProcessingResult::ack_all(count)) })
        });

        let consumer = NatsConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.*",
            10,
            5,
            processor,
        )
        .await
        .unwrap();

        let result = consumer.fetch_and_process_batch().await;
        assert!(result.is_ok());

        // Verify all messages were ack'd
        // (verification depends on Message mocking approach)
    }

    #[tokio::test]
    async fn test_fetch_and_process_nak_all() {
        // Similar structure but processor returns nak_all
        // Implementation follows same pattern as above
    }

    #[tokio::test]
    async fn test_fetch_and_process_partial_ack_nak() {
        // Test fine-grained control with ProcessingResult::new(vec![0, 2], vec![(1, Some("error".into()))])
        // Verify messages 0 and 2 are ack'd, message 1 is nak'd
    }

    #[tokio::test]
    async fn test_processor_error_handling() {
        // Processor returns an error
        let processor: BatchProcessor = Box::new(|_msgs| {
            Box::pin(async { Err(anyhow::anyhow!("Processing failed")) })
        });

        // Verify all messages are nak'd when processor errors
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        use tokio_util::sync::CancellationToken;

        let mock_jetstream = MockJetStreamConsumer::new();
        // Set up mock to return empty batches (simulate waiting)

        let processor: BatchProcessor = Box::new(|msgs| {
            Box::pin(async move { Ok(ProcessingResult::ack_all(msgs.len())) })
        });

        let consumer = NatsConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.*",
            10,
            5,
            processor,
        )
        .await
        .unwrap();

        let ctx = CancellationToken::new();
        let ctx_clone = ctx.clone();

        // Spawn consumer run in background
        let handle = tokio::spawn(async move {
            consumer.run(ctx_clone).await
        });

        // Wait a bit, then cancel
        tokio::time::sleep(Duration::from_millis(100)).await;
        ctx.cancel();

        // Verify consumer exits gracefully
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(result.is_ok(), "Consumer should exit within timeout");
    }
```

#### 3. Add README testing documentation
**File**: `crates/ponix-nats/README.md`
**Changes**: Add section on testing patterns

```markdown
## Testing

This crate uses [mockall](https://docs.rs/mockall/) for unit testing with mocked dependencies.

### Running Tests

```bash
# Run all tests
cargo test -p ponix-nats

# Run tests with feature flags
cargo test -p ponix-nats --features processed-envelope

# Run specific test module
cargo test -p ponix-nats consumer::tests
```

### Testing Patterns

The crate provides trait abstractions for JetStream operations:

- **`JetStreamConsumer`** - For consumer operations (create consumer, fetch messages)
- **`JetStreamPublisher`** - For publisher operations (stream management, publishing)

These traits are automatically mocked in test mode using mockall's `#[automock]` attribute.

#### Example: Testing with Mocks

```rust
use ponix_nats::traits::MockJetStreamPublisher;
use mockall::predicate::*;

#[tokio::test]
async fn test_publish() {
    let mut mock = MockJetStreamPublisher::new();

    mock.expect_publish()
        .with(eq("test.subject".to_string()), always())
        .times(1)
        .returning(|_, _| Ok(()));

    // Use mock in your test...
}
```

### Test Coverage

Unit tests cover:
- Connection and client creation
- Consumer creation and message fetching
- Message processing with ack/nak behavior
- Producer publishing and error handling
- ProcessedEnvelope serialization/deserialization
- Graceful shutdown scenarios
```

#### 4. Add lib.rs documentation
**File**: `crates/ponix-nats/src/lib.rs`
**Changes**: Add module-level documentation

```rust
//! # ponix-nats
//!
//! A NATS JetStream client library with support for batch message processing
//! and producer/consumer patterns.
//!
//! ## Features
//!
//! - **Generic batch consumer** - Process messages in batches with fine-grained ack/nak control
//! - **Protobuf support** - Optional ProcessedEnvelope producer/consumer (feature: `processed-envelope`)
//! - **Testable design** - Trait-based abstractions for easy unit testing with mocks
//! - **Graceful shutdown** - Coordinated shutdown using cancellation tokens
//!
//! ## Usage
//!
//! ```rust,no_run
//! use ponix_nats::{NatsClient, NatsConsumer, ProcessingResult, BatchProcessor};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create client and connect
//!     let client = NatsClient::connect("nats://localhost:4222", 5).await?;
//!
//!     // Ensure stream exists
//!     client.ensure_stream("my-stream").await?;
//!
//!     // Create custom processor
//!     let processor: BatchProcessor = Box::new(|messages| {
//!         Box::pin(async move {
//!             // Process messages...
//!             Ok(ProcessingResult::ack_all(messages.len()))
//!         })
//!     });
//!
//!     // Create consumer using trait-based API
//!     let consumer_client = client.create_consumer_client();
//!     let consumer = NatsConsumer::new(
//!         consumer_client,
//!         "my-stream",
//!         "my-consumer",
//!         "events.*",
//!         10,
//!         5,
//!         processor,
//!     ).await?;
//!
//!     // Run consumer (blocks until cancelled)
//!     let ctx = tokio_util::sync::CancellationToken::new();
//!     consumer.run(ctx).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Testing
//!
//! This crate provides trait abstractions (`JetStreamConsumer`, `JetStreamPublisher`) that
//! can be mocked using [mockall](https://docs.rs/mockall/) for unit testing without a NATS server.
//!
//! See the [README](../README.md) for more testing examples.

// Module declarations...
```

#### 5. Add trait documentation
**File**: `crates/ponix-nats/src/traits.rs`
**Changes**: Enhance trait documentation

```rust
//! Trait abstractions for JetStream operations
//!
//! These traits provide abstractions over async-nats JetStream operations,
//! allowing for easy testing with mocks using mockall.
//!
//! In production code, use the concrete implementations provided by `NatsClient`:
//! - `NatsJetStreamConsumer` - implements `JetStreamConsumer`
//! - `NatsJetStreamPublisher` - implements `JetStreamPublisher`
//!
//! In tests, use the auto-generated mocks:
//! - `MockJetStreamConsumer`
//! - `MockJetStreamPublisher`
//! - `MockPullConsumer`

// (Rest of traits.rs with existing code, just add this module doc at top)
```

### Success Criteria:

#### Automated Verification:
- [ ] All tests pass: `cargo test -p ponix-nats`
- [ ] All tests pass with features: `cargo test -p ponix-nats --features processed-envelope`
- [ ] Documentation builds: `cargo doc -p ponix-nats --no-deps`
- [ ] No clippy warnings: `cargo clippy -p ponix-nats --all-targets --all-features`
- [ ] Full workspace tests pass: `cargo test --workspace`
- [ ] README examples compile: `cargo test -p ponix-nats --doc`

#### Manual Verification:
- [ ] All consumer tests are implemented and passing
- [ ] Message mocking strategy is consistent across all tests
- [ ] Documentation is clear and helpful
- [ ] Code examples in docs are accurate
- [ ] Test coverage is comprehensive (happy paths and error cases)
- [ ] No TODO comments remain in the code

**Implementation Note**: After completing this phase and all automated verification passes, the implementation is complete. Verify all manual checks are satisfied before closing the issue.

---

## Testing Strategy

### Unit Tests

Each component has its own test module:

1. **Trait adapters** (client.rs):
   - Basic integration tests in Phase 2 verification
   - Detailed testing happens through consumer/producer tests

2. **NatsConsumer** (consumer.rs):
   - Consumer creation (success/failure)
   - Empty batch handling
   - Ack all messages
   - Nak all messages
   - Partial ack/nak (fine-grained control)
   - Processor error handling
   - Graceful shutdown

3. **ProcessedEnvelopeProducer** (processed_envelope_producer.rs):
   - Producer creation
   - Successful publish
   - Correct subject formatting
   - Protobuf serialization
   - Publish failures
   - Multiple envelopes
   - Different device IDs

4. **ProcessedEnvelopeProcessor** (processed_envelope_processor.rs):
   - Valid protobuf processing
   - Invalid protobuf handling
   - Multiple messages
   - Mixed valid/invalid batches
   - Empty batches

### Integration Testing

Integration tests with a real NATS server are OUT OF SCOPE for this issue. The unit tests with mocks ensure correctness of the business logic and error handling.

Future work could add:
- Integration tests using testcontainers-rs with NATS
- End-to-end tests in ponix-all-in-one
- Performance benchmarks

### Manual Testing Steps

After implementation:

1. **Verify ponix-all-in-one still works**:
   ```bash
   cargo build -p ponix-all-in-one
   # Optionally: Run with local NATS server
   ```

2. **Check test coverage**:
   ```bash
   cargo test -p ponix-nats -- --nocapture
   ```

3. **Review test output for clarity**:
   - Error messages should be descriptive
   - Test names should be clear
   - Failures should pinpoint the issue

4. **Documentation verification**:
   ```bash
   cargo doc -p ponix-nats --open
   ```
   - Check that traits, types, and examples render correctly
   - Verify links work

## Performance Considerations

- **Mock overhead**: Mockall mocks are zero-cost in production (only compiled in test mode)
- **Trait object overhead**: `Arc<dyn Trait>` has minimal overhead compared to direct async-nats calls
- **Testing performance**: Unit tests should run in milliseconds without NATS server

## Migration Notes

### For ponix-all-in-one users:

Update consumer creation:
```rust
// Before:
let consumer = NatsConsumer::new(
    client.jetstream(),  // Direct jetstream::Context
    // ...
).await?;

// After:
let consumer = NatsConsumer::new(
    client.create_consumer_client(),  // Arc<dyn JetStreamConsumer>
    // ...
).await?;
```

Update producer creation:
```rust
// Before:
let producer = ProcessedEnvelopeProducer::new(
    client.jetstream(),  // Direct jetstream::Context
    subject,
);

// After:
let producer = ProcessedEnvelopeProducer::new(
    client.create_publisher_client(),  // Arc<dyn JetStreamPublisher>
    subject,
);
```

### For custom integrations:

If you're using ponix-nats in other projects:
1. Update consumer/producer construction to use `create_consumer_client()` / `create_publisher_client()`
2. No changes needed to existing BatchProcessor implementations
3. Test your integration after updating

## References

- Original issue: [ponix-dev/ponix-rs#10](https://github.com/ponix-dev/ponix-rs/issues/10)
- Mockall documentation: https://docs.rs/mockall/
- async-nats documentation: https://docs.rs/async-nats/
- Existing test patterns: [runner/src/lib.rs:293-358](crates/runner/src/lib.rs#L293-L358)
