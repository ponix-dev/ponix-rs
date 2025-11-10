#[cfg(feature = "processed-envelope")]
use anyhow::Result;
#[cfg(feature = "processed-envelope")]
use async_nats::jetstream::Message;
#[cfg(feature = "processed-envelope")]
use crate::{BatchProcessor, ProcessingResult};
#[cfg(feature = "processed-envelope")]
use ponix_proto::envelope::v1::ProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use prost::Message as ProstMessage;
#[cfg(feature = "processed-envelope")]
use tracing::{debug, error, info};

/// Creates the default batch processor for ProcessedEnvelope messages
/// This processor deserializes protobuf messages and logs envelope details
/// Handles each message independently - successful messages are acked, failed ones are nakked
#[cfg(feature = "processed-envelope")]
pub fn create_processed_envelope_processor() -> BatchProcessor {
    Box::new(|messages: &[Message]| {
        // Clone the messages we need for the async block
        let messages = messages.to_vec();
        Box::pin(async move {
            debug!(batch_size = messages.len(), "Processing envelope batch");

            let mut ack = Vec::new();
            let mut nak = Vec::new();

            // Process each message independently
            for (idx, msg) in messages.iter().enumerate() {
                match ProcessedEnvelope::decode(msg.payload.as_ref()) {
                    Ok(envelope) => {
                        // Successfully deserialized, now process the envelope
                        info!(
                            end_device_id = %envelope.end_device_id,
                            organization_id = %envelope.organization_id,
                            occurred_at = ?envelope.occurred_at,
                            processed_at = ?envelope.processed_at,
                            "Processed envelope from NATS"
                        );
                        // Mark for acknowledgment
                        ack.push(idx);
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            subject = %msg.subject,
                            message_index = idx,
                            "Failed to deserialize ProcessedEnvelope"
                        );
                        // Mark for rejection with error details
                        nak.push((idx, Some(format!("Deserialization error: {}", e))));
                    }
                }
            }

            Ok(ProcessingResult::new(ack, nak))
        }) as futures::future::BoxFuture<'static, Result<ProcessingResult>>
    })
}

// Note: Unit tests for this processor would require mocking async_nats::jetstream::Message,
// which is complex. The processor logic is tested indirectly through:
// 1. Consumer tests that verify ProcessingResult handling
// 2. Integration tests with a real NATS server
// See Phase 6 for potential Message mocking strategies.
