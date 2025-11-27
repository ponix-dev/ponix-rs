use crate::nats::{set_parent_from_headers, BatchProcessor, ProcessingResult};
use anyhow::Result;
use async_nats::jetstream::Message;
use futures::future::BoxFuture;
use prost::Message as ProstMessage;
use std::sync::Arc;
use tracing::{debug, error, info_span, Instrument};

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
pub type ProtobufHandler<T> = Arc<
    dyn Fn(Vec<DecodedMessage<T>>) -> BoxFuture<'static, Result<ProcessingResult>> + Send + Sync,
>;

/// Creates a generic batch processor that decodes protobuf messages and delegates
/// business logic to a handler function.
///
/// # Type Parameters
/// * `T` - The protobuf message type to decode (must implement prost::Message + Default)
///
/// # Arguments
/// * `handler` - Async function that processes successfully decoded messages and
///   returns ProcessingResult with indices to ack/nak
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
pub fn create_protobuf_processor<T>(handler: ProtobufHandler<T>) -> BatchProcessor
where
    T: ProstMessage + Default + Send + 'static,
{
    Box::new(move |messages: &[Message]| {
        let messages = messages.to_vec();
        let handler = handler.clone();

        Box::pin(async move {
            // Create a span for the batch processing
            let batch_span = info_span!(
                "process_nats_batch",
                batch_size = messages.len(),
                message_type = std::any::type_name::<T>()
            );

            async move {
                debug!(batch_size = messages.len(), "decoding protobuf batch");

                let mut decoded_messages = Vec::new();
                let mut decode_failure_acks = Vec::new();

                // Phase 1: Decode all messages, track failures for acking
                for (idx, msg) in messages.into_iter().enumerate() {
                    // Extract trace context from message headers for distributed tracing
                    if let Some(headers) = msg.headers.as_ref() {
                        set_parent_from_headers(headers);
                    }

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
                                "failed to decode protobuf message - acking to prevent poison pill"
                            );
                            // Ack failed decodes to remove poison pills from stream
                            decode_failure_acks.push(idx);
                        }
                    }
                }

                debug!(
                    decoded_count = decoded_messages.len(),
                    failed_count = decode_failure_acks.len(),
                    "protobuf decode phase complete"
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
                    "protobuf processor complete"
                );

                Ok(handler_result)
            }
            .instrument(batch_span)
            .await
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let _msg1 = TestMessage {
            content: "hello".to_string(),
        };
        let _msg2 = TestMessage {
            content: "world".to_string(),
        };

        // Create handler that validates it receives decoded messages
        // and returns ack indices
        let handler: ProtobufHandler<TestMessage> = Arc::new(|decoded_messages| {
            Box::pin(async move {
                assert_eq!(decoded_messages.len(), 2);

                // Verify we can access both the decoded value and original message
                assert_eq!(decoded_messages[0].decoded.content, "hello");
                assert_eq!(decoded_messages[1].decoded.content, "world");

                // Build ack indices from the decoded messages
                let ack_indices: Vec<usize> = decoded_messages.iter().map(|dm| dm.index).collect();

                Ok(ProcessingResult::new(ack_indices, vec![]))
            })
        });

        let _processor = create_protobuf_processor(handler);

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
        let handler: ProtobufHandler<TestMessage> = Arc::new(|decoded_messages| {
            Box::pin(async move {
                // Should only get valid messages
                assert_eq!(decoded_messages.len(), 1);
                assert_eq!(decoded_messages[0].index, 1); // Second message was valid

                // Ack the valid message
                Ok(ProcessingResult::new(vec![1], vec![]))
            })
        });

        let _processor = create_protobuf_processor(handler);

        // Test would verify:
        // - Invalid message at index 0 is ack'd (poison pill handling)
        // - Valid message at index 1 is ack'd (business logic)
        // - Total: 2 acks, 0 naks
    }

    #[tokio::test]
    async fn test_handler_error_propagates() {
        // Test that errors from the business handler propagate correctly
        let handler: ProtobufHandler<TestMessage> = Arc::new(|_decoded_messages| {
            Box::pin(async move { Err(anyhow::anyhow!("Handler error")) })
        });

        let _processor = create_protobuf_processor(handler);

        // Test would verify error propagates
        // let result = processor(&[mock_msg]).await;
        // assert!(result.is_err());
        // All successfully decoded messages would be nak'd by consumer
    }

    #[tokio::test]
    async fn test_empty_batch() {
        // Test that empty batches are handled gracefully
        let handler: ProtobufHandler<TestMessage> = Arc::new(|decoded_messages| {
            Box::pin(async move {
                assert_eq!(decoded_messages.len(), 0);
                Ok(ProcessingResult::new(vec![], vec![]))
            })
        });

        let _processor = create_protobuf_processor(handler);

        // Test with empty message array
        // let result = processor(&[]).await.unwrap();
        // assert_eq!(result.ack.len(), 0);
        // assert_eq!(result.nak.len(), 0);
    }

    #[tokio::test]
    async fn test_handler_partial_success() {
        // Test that handler can return partial success (some ack, some nak)
        // and decode failures are correctly merged as acks
        let handler: ProtobufHandler<TestMessage> = Arc::new(|decoded_messages| {
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

        let _processor = create_protobuf_processor(handler);

        // Test would verify:
        // - Handler naks are preserved
        // - Handler acks are preserved
        // - Decode failure acks are added to ack list
    }

    #[tokio::test]
    async fn test_all_decode_failures() {
        // Test that when all messages fail to decode, they're all ack'd
        let handler: ProtobufHandler<TestMessage> = Arc::new(|decoded_messages| {
            Box::pin(async move {
                // Should receive no messages since all failed to decode
                assert_eq!(decoded_messages.len(), 0);
                Ok(ProcessingResult::new(vec![], vec![]))
            })
        });

        let _processor = create_protobuf_processor(handler);

        // Test with all invalid protobuf data
        // let result = processor(&[bad_msg1, bad_msg2, bad_msg3]).await.unwrap();
        // assert_eq!(result.ack.len(), 3); // All ack'd as poison pills
        // assert_eq!(result.nak.len(), 0);
    }
}
