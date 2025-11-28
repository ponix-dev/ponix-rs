use crate::domain::RawEnvelopeService;
use common::domain::RawEnvelope;
use common::nats::{ConsumeRequest, ConsumeResponse};
use common::proto::raw_envelope_proto_to_domain;
use futures::future::BoxFuture;
use ponix_proto::envelope::v1::RawEnvelope as ProtoRawEnvelope;
use prost::Message as ProstMessage;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;
use tracing::{debug, error, warn};

/// Tower service for processing individual RawEnvelope messages.
///
/// This service:
/// 1. Decodes the protobuf message
/// 2. Converts to domain type
/// 3. Processes through the domain RawEnvelopeService (which handles CEL conversion and publishing)
/// 4. Returns Ack/Nak response
#[derive(Clone)]
pub struct RawEnvelopeConsumerService {
    domain_service: Arc<RawEnvelopeService>,
}

impl RawEnvelopeConsumerService {
    pub fn new(domain_service: Arc<RawEnvelopeService>) -> Self {
        Self { domain_service }
    }
}

impl Service<ConsumeRequest> for RawEnvelopeConsumerService {
    type Response = ConsumeResponse;
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<ConsumeResponse, anyhow::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ConsumeRequest) -> Self::Future {
        let domain_service = Arc::clone(&self.domain_service);
        let subject = req.subject.clone();

        Box::pin(async move {
            // Decode protobuf
            let proto_envelope = match ProtoRawEnvelope::decode(&req.payload[..]) {
                Ok(envelope) => envelope,
                Err(e) => {
                    error!(
                        error = %e,
                        subject = %subject,
                        "failed to decode RawEnvelope protobuf"
                    );
                    return Ok(ConsumeResponse::nak(format!("Decode error: {}", e)));
                }
            };

            // Convert to domain type
            let domain_envelope: RawEnvelope = match raw_envelope_proto_to_domain(proto_envelope) {
                Ok(envelope) => envelope,
                Err(e) => {
                    error!(
                        error = %e,
                        subject = %subject,
                        "failed to convert protobuf to domain"
                    );
                    return Ok(ConsumeResponse::nak(format!("Conversion error: {}", e)));
                }
            };

            let device_id = domain_envelope.end_device_id.clone();

            // Process through domain service
            match domain_service.process_raw_envelope(domain_envelope).await {
                Ok(()) => {
                    debug!(
                        device_id = %device_id,
                        "successfully processed RawEnvelope"
                    );
                    Ok(ConsumeResponse::ack())
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        device_id = %device_id,
                        "failed to process RawEnvelope"
                    );
                    Ok(ConsumeResponse::nak(e.to_string()))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    // Domain service tests are comprehensive in raw_envelope_service.rs
    // Integration tests cover the full flow with real NATS
}
