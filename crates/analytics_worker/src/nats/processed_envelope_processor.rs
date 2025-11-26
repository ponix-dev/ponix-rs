use crate::domain::ProcessedEnvelopeService;
use anyhow::Result;
use common::domain::StoreEnvelopesInput;
use common::nats::{BatchProcessor, ProcessingResult};
use common::proto::processed_envelop_proto_to_domain;
use futures::future::BoxFuture;
use ponix_proto::envelope::v1::ProcessedEnvelope as ProtoEnvelope;
use std::sync::Arc;
use tracing::{debug, error};

/// Create a batch processor that converts protobuf envelopes to domain types
/// and stores them via the domain service
pub fn create_processed_envelope_processor(
    service: Arc<ProcessedEnvelopeService>,
) -> BatchProcessor {
    Box::new(move |messages: &[async_nats::jetstream::Message]| {
        let service = service.clone();

        // Decode protobuf messages immediately (while we still have access to the slice)
        use prost::Message as ProstMessage;

        let mut decoded_envelopes = Vec::new();
        let mut decode_errors = Vec::new();

        for (index, msg) in messages.iter().enumerate() {
            match ProtoEnvelope::decode(&msg.payload[..]) {
                Ok(proto_envelope) => decoded_envelopes.push((index, proto_envelope)),
                Err(e) => {
                    error!(
                        "Failed to decode protobuf message at index {}: {}",
                        index, e
                    );
                    decode_errors.push((index, Some(format!("Decode error: {}", e))));
                }
            }
        }

        Box::pin(async move {
            if decoded_envelopes.is_empty() {
                debug!("No ProcessedEnvelope messages to process");
                // Nak all messages that failed to decode
                return Ok(ProcessingResult::new(vec![], decode_errors));
            }

            // Convert protobuf to domain types
            let domain_envelopes: Result<Vec<_>> = decoded_envelopes
                .iter()
                .map(|(_, proto)| processed_envelop_proto_to_domain(proto.clone()))
                .collect();

            let domain_envelopes = match domain_envelopes {
                Ok(envelopes) => envelopes,
                Err(e) => {
                    error!("Failed to convert protobuf to domain: {}", e);
                    // Nak all messages that failed conversion
                    let mut nak_indices: Vec<(usize, Option<String>)> = decoded_envelopes
                        .iter()
                        .map(|(idx, _)| (*idx, Some(format!("Conversion error: {}", e))))
                        .collect();
                    nak_indices.extend(decode_errors);
                    return Ok(ProcessingResult::new(vec![], nak_indices));
                }
            };

            // Call domain service
            let input = StoreEnvelopesInput {
                envelopes: domain_envelopes,
            };

            match service.store_batch(input).await {
                Ok(()) => {
                    debug!(
                        envelope_count = decoded_envelopes.len(),
                        "Successfully processed envelope batch"
                    );
                    // Ack all successfully processed messages
                    let ack_indices: Vec<usize> =
                        decoded_envelopes.iter().map(|(idx, _)| *idx).collect();
                    Ok(ProcessingResult::new(ack_indices, decode_errors))
                }
                Err(e) => {
                    error!("Failed to store envelopes: {}", e);
                    // Nak all messages that failed storage
                    let mut nak_indices: Vec<(usize, Option<String>)> = decoded_envelopes
                        .iter()
                        .map(|(idx, _)| (*idx, Some(format!("Storage error: {}", e))))
                        .collect();
                    nak_indices.extend(decode_errors);
                    Ok(ProcessingResult::new(vec![], nak_indices))
                }
            }
        }) as BoxFuture<'static, Result<ProcessingResult>>
    })
}
