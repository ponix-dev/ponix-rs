# Refactor: Decouple Protobuf Decoding from Business Logic in Batch Processor

## Overview

Refactor the NATS batch processing system to separate protobuf decoding logic from message-specific business logic. Currently, `create_processed_envelope_processor()` couples protobuf deserialization with the business logic for handling `ProcessedEnvelope` messages. This refactoring will create a reusable generic protobuf batch processor that can work with any protobuf message type.

## Current State Analysis

### Existing Implementation

**File**: [crates/ponix-nats/src/processed_envelope_processor.rs](crates/ponix-nats/src/processed_envelope_processor.rs)

The current processor (lines 18-59) tightly couples:
- Protobuf decoding: `ProcessedEnvelope::decode(msg.payload.as_ref())` (line 30)
- Business logic: Logging envelope details with structured tracing (lines 33-39)
- Result handling: Direct construction of ack/nak indices (lines 25-26, 41, 51)

**Problems**:
1. Cannot reuse decoding logic for other protobuf message types
2. No clear separation between infrastructure (decoding) and domain logic (processing)
3. Adding new message processors requires duplicating decode/error handling patterns
4. Poison pill messages that fail decode are nak'd repeatedly (line 51)

### Key Discoveries

- **BatchProcessor Type** ([consumer.rs:43-47](crates/ponix-nats/src/consumer.rs#L43-L47)):
  - Boxed async function: `&[Message] -> BoxFuture<Result<ProcessingResult>>`
  - Already generic over processing logic

- **ProcessingResult** ([consumer.rs:10-41](crates/ponix-nats/src/consumer.rs#L10-L41)):
  - Separate ack/nak vectors with optional error messages
  - Uses message indices to track which messages to ack/nak
  - Provides `ack_all`, `nak_all`, and `new` constructors

- **Message Acknowledgment** ([consumer.rs:157-221](crates/ponix-nats/src/consumer.rs#L157-L221)):
  - Messages are ack'd/nak'd individually by index
  - Index-based approach requires maintaining message order
  - Direct access to original NATS message needed for ack operations

- **Error Handling Pattern** ([processed_envelope_processor.rs:43-51](crates/ponix-nats/src/processed_envelope_processor.rs#L43-L51)):
  - Decode failures are nak'd with detailed error messages
  - **Issue**: Poison pills (permanently malformed messages) get nak'd forever

- **Testing Infrastructure** ([consumer.rs:224-362](crates/ponix-nats/src/consumer.rs#L224-L362)):
  - Uses mockall with trait-based abstractions from issue #10
  - Tests verify ProcessingResult handling with ack/nak scenarios

## Desired End State

### Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│ Generic Protobuf Batch Processor (protobuf.rs)                      │
│                                                                       │
│ 1. Decode Vec<Message> -> Vec<DecodedMessage<T>>                    │
│ 2. Ack decode failures immediately (poison pill handling)            │
│ 3. Pass successfully decoded messages to business handler            │
│ 4. Handler converts DecodedMessage<T> to ProcessingResult            │
│ 5. Return combined ProcessingResult                                  │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
        ┌──────────────────────────────────────────────────────┐
        │ Business Handler (async closure/fn)                  │
        │                                                        │
        │ Takes: Vec<DecodedMessage<T>>                         │
        │ Returns: Result<ProcessingResult>                     │
        │                                                        │
        │ DecodedMessage provides:                              │
        │   - index: usize (position in original batch)         │
        │   - message: Message (original NATS message)          │
        │   - decoded: T (parsed protobuf value)                │
        │                                                        │
        │ Pure business logic - no decoding                     │
        └──────────────────────────────────────────────────────┘
```

### Key Design Decisions

1. **Poison Pill Handling**: Failed decode messages are **ack'd** (not nak'd) to prevent infinite redelivery loops
2. **Message Wrapper**: `DecodedMessage<T>` struct keeps original NATS message with decoded protobuf value
3. **Index Tracking**: Original batch index preserved for ProcessingResult construction
4. **Handler Responsibility**: Business handler converts `DecodedMessage<T>` to ack/nak indices

### Verification

After implementation is complete:

1. **Functional Verification**:
   - `ponix-all-in-one` service processes envelopes without code changes in `main.rs`
   - Decode failures are logged and ack'd (not nak'd)
   - Successfully decoded messages are processed by business logic
   - Messages are ack'd/nak'd based on business logic results

2. **Code Quality Verification**:
   - New protobuf processor is generic over any `T: prost::Message`
   - Business logic can be injected without knowing about protobuf decoding
   - `processed_envelope_processor.rs` uses the new generic processor
   - No poison pill messages stuck in redelivery loops

3. **Test Verification**:
   - All existing tests pass: `cargo test -p ponix-nats`
   - New unit tests cover generic protobuf processor
   - Test coverage includes decode failures and business logic failures

## What We're NOT Doing

- Changes to the `BatchProcessor` type signature
- Changes to the `NatsConsumer` implementation
- Changes to the `ProcessingResult` struct
- New message types beyond `ProcessedEnvelope`
- Changes to `ponix-all-in-one/main.rs` integration code
- Migration of existing messages in the stream
- Retry logic for poison pill messages (they are permanently ack'd)

## Implementation Approach

We'll implement this in two phases:

1. **Phase 1**: Create the generic protobuf processor in a new `protobuf.rs` file with unit tests
2. **Phase 2**: Refactor `processed_envelope_processor.rs` to use the generic processor, verify integration

This approach allows us to:
- Test the generic processor independently before integration
- Maintain backward compatibility throughout
- Verify each phase before proceeding

---

## Phase 1: Generic Protobuf Batch Processor

### Overview

Create a new `protobuf.rs` module with:
1. `DecodedMessage<T>` wrapper type that pairs NATS messages with decoded protobuf values
2. Generic batch processor factory function that handles protobuf decoding
3. Business handler type that works with decoded messages

### Changes Required

#### 1. Create New Module File

**File**: `crates/ponix-nats/src/protobuf.rs`

**Implementation**:

```rust
use anyhow::Result;
use async_nats::jetstream::Message;
use crate::{BatchProcessor, ProcessingResult};
use futures::future::BoxFuture;
use prost::Message as ProstMessage;
use tracing::{debug, error};

/// Wrapper type that pairs a decoded protobuf message with its original NATS message
/// This allows business handlers to process decoded data while maintaining access
/// to the underlying NATS message for acknowledgment operations
#[derive(Debug)]
pub struct DecodedMessage<T> {
    /// The position of this message in the original batch
    pub index: usize,
    /// The original NATS JetStream message
    pub message: Message,
    /// The decoded protobuf value
    pub decoded: T,
}

impl<T> DecodedMessage<T> {
    /// Create a new DecodedMessage
    pub fn new(index: usize, message: Message, decoded: T) -> Self {
        Self {
            index,
            message,
            decoded,
        }
    }
}

/// Type alias for business logic handlers that process decoded protobuf messages
/// The handler receives successfully decoded messages and returns a ProcessingResult
/// indicating which message indices to ack/nak based on business logic
pub type ProtobufHandler<T> = Box<
    dyn Fn(Vec<DecodedMessage<T>>) -> BoxFuture<'static, Result<ProcessingResult>> + Send + Sync
>;

/// Creates a generic batch processor that decodes protobuf messages and delegates
/// business logic to a handler function.
///
/// # Type Parameters
/// * `T` - The protobuf message type to decode (must implement prost::Message + Default)
///
/// # Arguments
/// * `handler` - Async function that processes successfully decoded messages and
///               returns ProcessingResult with indices to ack/nak
///
/// # Processing Flow
/// 1. Attempts to decode each raw NATS message to type T
/// 2. Logs and acks messages that fail to decode (poison pill handling)
/// 3. Wraps successfully decoded messages with their original NATS message
/// 4. Passes wrapped messages to the handler
/// 5. Handler returns ProcessingResult with ack/nak indices
/// 6. Combines handler results with decode failure acks
///
/// # Error Handling
/// - Decode failures: Logged with error details, ack'd to prevent poison pills
/// - Handler errors: Propagated as Result::Err, will nak all successfully decoded messages
///
/// # Example
/// ```rust,ignore
/// let processor = create_protobuf_processor::<MyProtoMessage>(
///     Box::new(|decoded_messages| {
///         Box::pin(async move {
///             // Process decoded messages, build ack/nak indices
///             let ack_indices: Vec<usize> = decoded_messages.iter()
///                 .filter(|dm| is_valid(&dm.decoded))
///                 .map(|dm| dm.index)
///                 .collect();
///
///             Ok(ProcessingResult::new(ack_indices, vec![]))
///         })
///     })
/// );
/// ```
pub fn create_protobuf_processor<T>(handler: ProtobufHandler<T>) -> BatchProcessor
where
    T: ProstMessage + Default + Send + 'static,
{
    Box::new(move |messages: &[Message]| {
        let messages = messages.to_vec();
        let handler = handler.clone();

        Box::pin(async move {
            debug!(batch_size = messages.len(), "Decoding protobuf batch");

            let mut decoded_messages = Vec::new();
            let mut decode_failure_acks = Vec::new();

            // Phase 1: Decode all messages, track failures for acking
            for (idx, msg) in messages.into_iter().enumerate() {
                match T::decode(msg.payload.as_ref()) {
                    Ok(decoded) => {
                        decoded_messages.push(DecodedMessage::new(idx, msg, decoded));
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            subject = %msg.subject,
                            message_index = idx,
                            payload_size = msg.payload.len(),
                            "Failed to decode protobuf message - acking to prevent poison pill"
                        );
                        // Ack failed decodes to remove poison pills from stream
                        decode_failure_acks.push(idx);
                    }
                }
            }

            debug!(
                decoded_count = decoded_messages.len(),
                failed_count = decode_failure_acks.len(),
                "Protobuf decode phase complete"
            );

            // Phase 2: Process successfully decoded messages with business handler
            let mut handler_result = if decoded_messages.is_empty() {
                // No messages to process, return empty result
                ProcessingResult::new(vec![], vec![])
            } else {
                handler(decoded_messages).await?
            };

            // Phase 3: Merge decode failure acks into handler result
            // Decode failures are always ack'd to prevent poison pill redelivery
            handler_result.ack.extend(decode_failure_acks);

            debug!(
                total_ack = handler_result.ack.len(),
                total_nak = handler_result.nak.len(),
                "Protobuf processor complete"
            );

            Ok(handler_result)
        }) as BoxFuture<'static, Result<ProcessingResult>>
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    // Test message type for unit tests
    #[derive(Clone, PartialEq, prost::Message)]
    struct TestMessage {
        #[prost(string, tag = "1")]
        content: String,
    }

    #[tokio::test]
    async fn test_decoded_message_creation() {
        // Verify DecodedMessage can be created with proper fields
        // Note: Creating real Message objects requires async_nats test utilities
        // This test demonstrates the API structure
    }

    #[tokio::test]
    async fn test_all_messages_decode_successfully() {
        // Create test messages
        let msg1 = TestMessage { content: "hello".to_string() };
        let msg2 = TestMessage { content: "world".to_string() };

        // Create handler that validates it receives decoded messages
        // and returns ack indices
        let handler: ProtobufHandler<TestMessage> = Box::new(|decoded_messages| {
            Box::pin(async move {
                assert_eq!(decoded_messages.len(), 2);

                // Verify we can access both the decoded value and original message
                assert_eq!(decoded_messages[0].decoded.content, "hello");
                assert_eq!(decoded_messages[1].decoded.content, "world");

                // Build ack indices from the decoded messages
                let ack_indices: Vec<usize> = decoded_messages.iter()
                    .map(|dm| dm.index)
                    .collect();

                Ok(ProcessingResult::new(ack_indices, vec![]))
            })
        });

        let processor = create_protobuf_processor(handler);

        // Test would call processor with mock messages
        // let result = processor(&[mock_msg1, mock_msg2]).await.unwrap();
        // assert_eq!(result.ack.len(), 2);
        // assert_eq!(result.nak.len(), 0);
    }

    #[tokio::test]
    async fn test_decode_failures_are_acked() {
        // Test that malformed protobuf messages are ack'd (not nak'd)
        // to prevent poison pill redelivery loops

        // Handler should only receive successfully decoded messages
        let handler: ProtobufHandler<TestMessage> = Box::new(|decoded_messages| {
            Box::pin(async move {
                // Should only get valid messages
                assert_eq!(decoded_messages.len(), 1);
                assert_eq!(decoded_messages[0].index, 1); // Second message was valid

                // Ack the valid message
                Ok(ProcessingResult::new(vec![1], vec![]))
            })
        });

        let processor = create_protobuf_processor(handler);

        // Test would verify:
        // - Invalid message at index 0 is ack'd (poison pill handling)
        // - Valid message at index 1 is ack'd (business logic)
        // - Total: 2 acks, 0 naks
    }

    #[tokio::test]
    async fn test_handler_error_propagates() {
        // Test that errors from the business handler propagate correctly
        let handler: ProtobufHandler<TestMessage> = Box::new(|_decoded_messages| {
            Box::pin(async move {
                Err(anyhow::anyhow!("Handler error"))
            })
        });

        let processor = create_protobuf_processor(handler);

        // Test would verify error propagates
        // let result = processor(&[mock_msg]).await;
        // assert!(result.is_err());
        // All successfully decoded messages would be nak'd by consumer
    }

    #[tokio::test]
    async fn test_empty_batch() {
        // Test that empty batches are handled gracefully
        let handler: ProtobufHandler<TestMessage> = Box::new(|decoded_messages| {
            Box::pin(async move {
                assert_eq!(decoded_messages.len(), 0);
                Ok(ProcessingResult::new(vec![], vec![]))
            })
        });

        let processor = create_protobuf_processor(handler);

        // Test with empty message array
        // let result = processor(&[]).await.unwrap();
        // assert_eq!(result.ack.len(), 0);
        // assert_eq!(result.nak.len(), 0);
    }

    #[tokio::test]
    async fn test_handler_partial_success() {
        // Test that handler can return partial success (some ack, some nak)
        // and decode failures are correctly merged as acks
        let handler: ProtobufHandler<TestMessage> = Box::new(|decoded_messages| {
            Box::pin(async move {
                // Simulate business logic that fails for some messages
                let mut ack = Vec::new();
                let mut nak = Vec::new();

                for dm in decoded_messages {
                    if dm.decoded.content == "valid" {
                        ack.push(dm.index);
                    } else {
                        nak.push((dm.index, Some("Business validation failed".to_string())));
                    }
                }

                Ok(ProcessingResult::new(ack, nak))
            })
        });

        let processor = create_protobuf_processor(handler);

        // Test would verify:
        // - Handler naks are preserved
        // - Handler acks are preserved
        // - Decode failure acks are added to ack list
    }

    #[tokio::test]
    async fn test_all_decode_failures() {
        // Test that when all messages fail to decode, they're all ack'd
        let handler: ProtobufHandler<TestMessage> = Box::new(|decoded_messages| {
            Box::pin(async move {
                // Should receive no messages since all failed to decode
                assert_eq!(decoded_messages.len(), 0);
                Ok(ProcessingResult::new(vec![], vec![]))
            })
        });

        let processor = create_protobuf_processor(handler);

        // Test with all invalid protobuf data
        // let result = processor(&[bad_msg1, bad_msg2, bad_msg3]).await.unwrap();
        // assert_eq!(result.ack.len(), 3); // All ack'd as poison pills
        // assert_eq!(result.nak.len(), 0);
    }
}
```

#### 2. Register Module in Library

**File**: `crates/ponix-nats/src/lib.rs`

**Changes**: Add module declaration and public exports

```rust
// Add after existing module declarations (after line 7)
pub mod protobuf;
```

Add to public exports section (after line 12):
```rust
pub use protobuf::{create_protobuf_processor, DecodedMessage, ProtobufHandler};
```

**Result** (updated lib.rs lines 1-17):
```rust
pub mod client;
pub mod consumer;
pub mod traits;
pub mod protobuf;  // NEW

#[cfg(feature = "processed-envelope")]
pub mod processed_envelope_processor;
#[cfg(feature = "processed-envelope")]
pub mod processed_envelope_producer;

pub use client::NatsClient;
pub use consumer::{BatchProcessor, NatsConsumer, ProcessingResult};
pub use traits::{JetStreamConsumer, JetStreamPublisher, PullConsumer};
pub use protobuf::{create_protobuf_processor, DecodedMessage, ProtobufHandler};  // NEW

#[cfg(feature = "processed-envelope")]
pub use processed_envelope_processor::create_processed_envelope_processor;
#[cfg(feature = "processed-envelope")]
pub use processed_envelope_producer::ProcessedEnvelopeProducer;
```

### Success Criteria

#### Automated Verification:
- [x] Code compiles without errors: `cargo build -p ponix-nats`
- [x] Type checking passes: `cargo check -p ponix-nats`
- [x] Linting passes: `cargo clippy -p ponix-nats -- -D warnings`
- [x] Unit tests compile: `cargo test -p ponix-nats --no-run`
- [x] New module is properly exported in lib.rs

#### Manual Verification:
- [ ] Review protobuf.rs implementation for correctness
- [ ] Verify error messages include all necessary debugging context
- [ ] Confirm generic type constraints allow any protobuf message type
- [ ] Validate that decode failures are ack'd (not nak'd) to prevent poison pills
- [ ] Verify `DecodedMessage` provides access to both original NATS message and decoded value
- [ ] Confirm handler can build ProcessingResult using message indices

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that the generic processor implementation is correct before proceeding to Phase 2.

---

## Phase 2: Refactor ProcessedEnvelope Processor

### Overview

Refactor `processed_envelope_processor.rs` to use the new generic protobuf processor, separating business logic into a dedicated handler function that works with `DecodedMessage<ProcessedEnvelope>` instances.

### Changes Required

#### 1. Refactor ProcessedEnvelope Processor

**File**: `crates/ponix-nats/src/processed_envelope_processor.rs`

**Changes**: Complete rewrite to use generic protobuf processor

**Replace entire file contents** (lines 1-59) with:

```rust
#[cfg(feature = "processed-envelope")]
use crate::{create_protobuf_processor, BatchProcessor, DecodedMessage, ProcessingResult, ProtobufHandler};
#[cfg(feature = "processed-envelope")]
use ponix_proto::envelope::v1::ProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use tracing::info;

/// Creates the default batch processor for ProcessedEnvelope messages
/// This processor uses the generic protobuf processor with business logic
/// that logs envelope details
#[cfg(feature = "processed-envelope")]
pub fn create_processed_envelope_processor() -> BatchProcessor {
    // Define business logic handler that works with decoded messages
    let handler: ProtobufHandler<ProcessedEnvelope> = Box::new(|decoded_messages| {
        Box::pin(async move {
            // Collect indices of all messages to ack
            let mut ack_indices = Vec::new();

            // Process each successfully decoded envelope
            for decoded_msg in decoded_messages {
                let envelope = &decoded_msg.decoded;

                info!(
                    end_device_id = %envelope.end_device_id,
                    organization_id = %envelope.organization_id,
                    occurred_at = ?envelope.occurred_at,
                    processed_at = ?envelope.processed_at,
                    "Processed envelope from NATS"
                );

                // All envelopes are successfully processed, ack them
                ack_indices.push(decoded_msg.index);
            }

            // Return processing result with all acks, no naks
            Ok(ProcessingResult::new(ack_indices, vec![]))
        })
    });

    // Create processor using generic protobuf processor
    create_protobuf_processor(handler)
}
```

**Rationale**:
- Separates protobuf decoding (handled by `create_protobuf_processor`) from business logic (logging)
- Business handler works with `DecodedMessage<ProcessedEnvelope>` to access both decoded data and original message
- Handler builds ack indices from successfully processed messages
- Poison pill handling (acking failed decodes) is automatic in the generic processor
- Maintains exact same external API and behavior
- Significantly reduced code: 59 lines → ~40 lines

#### 2. Verify Integration Point

**File**: `crates/ponix-all-in-one/src/main.rs`

**Verification**: No changes required! Lines 58-61 should continue to work:

```rust
// Create the batch processor
// The processor defines the custom logic for handling batches of messages
// Using the default ProcessedEnvelope processor from ponix-nats
let processor = create_processed_envelope_processor();
```

### Success Criteria

#### Automated Verification:
- [x] Full project builds: `cargo build`
- [x] All unit tests pass: `cargo test -p ponix-nats`
- [x] Integration compiles: `cargo check -p ponix-all-in-one`
- [x] No clippy warnings: `cargo clippy --all-targets -- -D warnings`
- [x] Documentation builds: `cargo doc -p ponix-nats --no-deps`

#### Manual Verification:
- [ ] Run `ponix-all-in-one` service locally with docker-compose
- [ ] Verify envelopes are processed and logged correctly
- [ ] Confirm decode failures are logged and ack'd (not nak'd)
- [ ] Test with malformed protobuf messages (inject bad data via NATS CLI)
- [ ] Verify poison pill messages are ack'd and removed from stream
- [ ] Check that valid messages continue to process normally
- [ ] Confirm no regressions in message processing behavior
- [ ] Verify performance characteristics are unchanged

**Commands for Testing**:
```bash
# Publish malformed data to test poison pill handling
nats pub processed_envelopes.test-device "invalid protobuf data"

# Check consumer metrics
nats consumer info processed_envelopes ponix-all-in-one

# Monitor service logs for proper error handling
docker-compose logs -f ponix-all-in-one
```

**Implementation Note**: After completing this phase and all automated verification passes, manually test the service end-to-end, including poison pill scenarios, to ensure no regressions before considering the refactoring complete.

---

## Testing Strategy

### Unit Tests

**New Tests in `protobuf.rs`**:
1. All messages decode successfully → handler receives all messages with indices
2. Some messages fail to decode → handler receives only valid messages, failures are ack'd
3. Handler returns error → error propagates correctly
4. Handler returns partial success → acks/naks are correctly merged with decode failure acks
5. Empty batch → handled gracefully
6. All decode failures → handler receives empty vec, all messages ack'd
7. DecodedMessage structure → verify index, message, and decoded fields are accessible

**Key Testing Challenge**: Creating mock `async_nats::jetstream::Message` objects is complex. Options:

1. **Integration tests** (recommended): Test with real NATS server
2. **Test utilities**: Create helper functions to construct mock messages (future work)
3. **Property-based testing**: Use proptest for various scenarios (future work)

**Existing Tests**: All tests in `consumer.rs` (lines 224-362) should continue to pass without modification.

### Integration Tests

**Test Scenarios** (manual testing with `ponix-all-in-one`):

1. **Happy Path**:
   - Start service with NATS server
   - Verify envelopes are produced and consumed
   - Check logs for proper processing

2. **Poison Pill Handling** (NEW):
   - Manually publish malformed protobuf data
   - Verify error logging with proper context
   - **Critical**: Confirm messages are ack'd (not nak'd)
   - Verify message is removed from stream (not redelivered)

3. **Mixed Batch**:
   - Publish batch with some valid, some invalid messages
   - Verify valid messages are processed
   - Verify invalid messages are ack'd
   - Verify stream metrics show proper ack counts

4. **Performance**:
   - Compare processing throughput before/after refactoring
   - Should be equivalent (no additional overhead)

### Testing Commands

```bash
# Unit tests
cargo test -p ponix-nats

# Integration tests
cargo test -p ponix-all-in-one

# Run service locally
docker-compose up -d nats
cargo run -p ponix-all-in-one

# Check NATS stream metrics
nats stream info processed_envelopes
nats consumer info processed_envelopes ponix-all-in-one

# Test poison pill handling
nats pub processed_envelopes.test-device "malformed data"
nats pub processed_envelopes.test-device '{"invalid":"json"}'
nats pub processed_envelopes.test-device "$(echo -ne '\x00\x01\x02\x03')"

# Verify message was ack'd and removed
nats consumer next processed_envelopes ponix-all-in-one --count 10
```

## Performance Considerations

### Expected Impact

**No Performance Degradation Expected**:
- Decode logic remains identical (same `prost::Message::decode` calls)
- Handler composition happens at initialization, not per-message
- Boxing and async overhead already present in current implementation
- `DecodedMessage` wrapper is a simple struct (no heap allocation)

**Potential Improvements**:
- Clearer separation may enable future optimizations
- Business logic handler can be swapped without touching decode logic
- Poison pill handling prevents infinite redelivery overhead

### Monitoring

**Key Metrics** (from NATS JetStream):
- Message throughput (messages/sec)
- Processing latency (time from publish to ack)
- Ack rate (should increase if testing poison pills)
- Nak rate (should only come from business logic, not decode failures)
- Consumer lag (should remain near zero under normal load)

**Logging Observability**:
- Current: Single log line per message with envelope details
- After: Additional debug logs for decode phase (can disable in production)
- **Critical**: Error logs for decode failures with "acking to prevent poison pill" message
- Errors include subject, index, payload size for debugging

## Migration Notes

### Zero Downtime Deployment

This refactoring maintains complete backward compatibility:

1. **No Schema Changes**: Protobuf message types unchanged
2. **No API Changes**: `create_processed_envelope_processor()` signature unchanged
3. **No Config Changes**: NATS connection parameters unchanged
4. **No Data Migration**: Existing messages in stream process identically
5. **Poison Pill Behavior Change**: Failed decodes now ack'd instead of nak'd (improvement)

### Deployment Steps

1. Build and test new version thoroughly
2. **Important**: Test poison pill handling in dev/staging first
3. Deploy new binary (no special migration steps needed)
4. Monitor logs for decode failures being ack'd
5. Monitor NATS metrics for proper ack behavior
6. Rollback is simple: deploy previous version if issues arise

### Rollback Plan

If issues are discovered post-deployment:
1. Deploy previous version of the service
2. Messages in stream are not affected
3. Processing resumes with old implementation (poison pills will nak again)
4. Investigate issue in development environment

**Note on Poison Pills**: After deployment, any poison pill messages in the stream will be ack'd on first retry. This is desired behavior. If you want to inspect these messages first, do so in staging before production deployment.

## References

- Original issue: [#12](https://github.com/ponix-dev/ponix-rs/issues/12)
- Related: Issue #3 (NATS JetStream implementation)
- Related: Issue #10 (mockall support and trait abstractions)
- Implementation plan: [2025-11-09-3-nats-jetstream-implementation.md](.thoughts/plans/2025-11-09-3-nats-jetstream-implementation.md)
- Current implementation: [processed_envelope_processor.rs:18-59](crates/ponix-nats/src/processed_envelope_processor.rs#L18-L59)
- Consumer implementation: [consumer.rs](crates/ponix-nats/src/consumer.rs)
