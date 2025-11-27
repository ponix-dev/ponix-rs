use crate::domain::RawEnvelopeService;
use async_nats::jetstream::Message;
use common::nats::{BatchProcessor, ProcessingResult};
use std::sync::Arc;
use tracing::{debug, error, warn};

/// Create a BatchProcessor for RawEnvelope messages that processes them through the domain service
pub fn create_raw_envelope_processor(service: Arc<RawEnvelopeService>) -> BatchProcessor {
    Box::new(move |messages: &[Message]| {
        let service = Arc::clone(&service);

        // Extract message payloads and subjects before moving into async block
        // This is necessary because Message borrows from the slice
        use prost::Message as ProstMessage;

        let message_data: Vec<(usize, Vec<u8>, String)> = messages
            .iter()
            .enumerate()
            .map(|(idx, msg)| (idx, msg.payload.to_vec(), msg.subject.to_string()))
            .collect();

        Box::pin(async move {
            let mut ack = Vec::new();
            let mut nak = Vec::new();

            // Single loop: decode, convert, and process each message
            for (idx, payload, subject) in message_data {
                // Decode protobuf message
                let proto_envelope =
                    match ponix_proto::envelope::v1::RawEnvelope::decode(&payload[..]) {
                        Ok(envelope) => envelope,
                        Err(e) => {
                            error!(
                                error = %e,
                                subject = %subject,
                                "failed to decode RawEnvelope protobuf message"
                            );
                            nak.push((idx, Some(format!("Decode error: {}", e))));
                            continue;
                        }
                    };

                // Convert to domain type
                let domain_envelope = match common::proto::raw_envelope_proto_to_domain(proto_envelope) {
                    Ok(envelope) => envelope,
                    Err(e) => {
                        error!(
                            error = %e,
                            subject = %subject,
                            "failed to convert protobuf RawEnvelope to domain type"
                        );
                        nak.push((idx, Some(format!("Conversion error: {}", e))));
                        continue;
                    }
                };

                // Process through domain service
                match service.process_raw_envelope(domain_envelope).await {
                    Ok(()) => {
                        debug!(index = idx, "successfully processed RawEnvelope");
                        ack.push(idx);
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            index = idx,
                            "failed to process RawEnvelope through domain service"
                        );
                        nak.push((idx, Some(e.to_string())));
                    }
                }
            }

            Ok(ProcessingResult { ack, nak })
        })
    })
}

// Note: Unit tests for the processor are challenging because we cannot easily create
// actual NATS Message objects without a real NATS connection.
// This functionality is better tested via integration tests with real NATS infrastructure.
// See the integration tests in the ponix_all_in_one crate for end-to-end testing.
