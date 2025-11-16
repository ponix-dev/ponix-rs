#[cfg(feature = "processed-envelope")]
use crate::{create_protobuf_processor, BatchProcessor, ProcessingResult, ProtobufHandler};
#[cfg(feature = "processed-envelope")]
use ponix_proto::envelope::v1::ProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use std::sync::Arc;
#[cfg(feature = "processed-envelope")]
use tracing::{error, info};

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

/// Creates a batch processor that writes ProcessedEnvelope messages to ClickHouse
/// This is a generic handler that can be used with any storage implementation
#[cfg(feature = "processed-envelope")]
pub fn create_clickhouse_processor<F, Fut>(store_fn: F) -> BatchProcessor
where
    F: Fn(Vec<ProcessedEnvelope>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let store_fn = Arc::new(store_fn);

    let handler: ProtobufHandler<ProcessedEnvelope> = Arc::new(move |decoded_messages| {
        let store_fn = Arc::clone(&store_fn);
        Box::pin(async move {
            // Extract envelopes from decoded messages
            let envelopes: Vec<ProcessedEnvelope> = decoded_messages
                .iter()
                .map(|msg| msg.decoded.clone())
                .collect();

            let message_count = envelopes.len();

            // Store to ClickHouse
            match store_fn(envelopes).await {
                Ok(_) => {
                    info!(
                        "Successfully stored {} envelopes to ClickHouse",
                        message_count
                    );
                    // Ack all messages on success
                    let ack_indices: Vec<usize> =
                        decoded_messages.iter().map(|msg| msg.index).collect();
                    Ok(ProcessingResult::new(ack_indices, vec![]))
                }
                Err(e) => {
                    error!("Failed to store envelopes to ClickHouse: {}", e);
                    // Nack all messages on failure so they can be retried
                    let error_msg = format!("ClickHouse error: {}", e);
                    let nak_indices: Vec<(usize, Option<String>)> = decoded_messages
                        .iter()
                        .map(|msg| (msg.index, Some(error_msg.clone())))
                        .collect();
                    Ok(ProcessingResult::new(vec![], nak_indices))
                }
            }
        })
    });

    create_protobuf_processor(handler)
}
