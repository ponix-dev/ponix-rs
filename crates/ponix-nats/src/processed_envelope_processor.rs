#[cfg(feature = "processed-envelope")]
use crate::{create_protobuf_processor, BatchProcessor, ProcessingResult, ProtobufHandler};
#[cfg(feature = "processed-envelope")]
use ponix_proto::envelope::v1::ProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use std::sync::Arc;
#[cfg(feature = "processed-envelope")]
use tracing::info;

/// Creates the default batch processor for ProcessedEnvelope messages
/// This processor uses the generic protobuf processor with business logic
/// that logs envelope details
#[cfg(feature = "processed-envelope")]
pub fn create_processed_envelope_processor() -> BatchProcessor {
    // Define business logic handler that works with decoded messages
    let handler: ProtobufHandler<ProcessedEnvelope> = Arc::new(|decoded_messages| {
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
