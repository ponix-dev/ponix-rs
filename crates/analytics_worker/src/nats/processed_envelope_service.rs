use crate::clickhouse::ClickHouseInserterRepository;
use common::domain::ProcessedEnvelope;
use common::nats::{ConsumeRequest, ConsumeResponse};
use common::proto::processed_envelop_proto_to_domain;
use futures::future::BoxFuture;
use ponix_proto_prost::envelope::v1::ProcessedEnvelope as ProtoEnvelope;
use prost::Message;
use std::task::{Context, Poll};
use tower::Service;
use tracing::{debug, error};

/// Tower service for processing individual ProcessedEnvelope messages.
///
/// This service:
/// 1. Decodes the protobuf message
/// 2. Converts to domain type
/// 3. Writes to ClickHouse via the inserter (buffered, fast)
/// 4. Returns Ack/Nak response
#[derive(Clone)]
pub struct ProcessedEnvelopeService {
    inserter: ClickHouseInserterRepository,
}

impl ProcessedEnvelopeService {
    pub fn new(inserter: ClickHouseInserterRepository) -> Self {
        Self { inserter }
    }
}

impl Service<ConsumeRequest> for ProcessedEnvelopeService {
    type Response = ConsumeResponse;
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<ConsumeResponse, anyhow::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ConsumeRequest) -> Self::Future {
        let inserter = self.inserter.clone();
        let subject = req.subject.clone();

        Box::pin(async move {
            // Decode protobuf
            let proto_envelope = match ProtoEnvelope::decode(&req.payload[..]) {
                Ok(envelope) => envelope,
                Err(e) => {
                    error!(
                        error = %e,
                        subject = %subject,
                        "failed to decode ProcessedEnvelope protobuf"
                    );
                    return Ok(ConsumeResponse::nak(format!("Decode error: {}", e)));
                }
            };

            // Convert to domain type
            let domain_envelope: ProcessedEnvelope =
                match processed_envelop_proto_to_domain(proto_envelope) {
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

            // Write to inserter buffer (fast - just buffers, doesn't flush)
            if let Err(e) = inserter.write(&domain_envelope).await {
                error!(
                    error = %e,
                    data_stream_id = %domain_envelope.data_stream_id,
                    "failed to buffer envelope for ClickHouse"
                );
                return Ok(ConsumeResponse::nak(format!("Buffer error: {}", e)));
            }

            debug!(
                data_stream_id = %domain_envelope.data_stream_id,
                "buffered ProcessedEnvelope"
            );

            Ok(ConsumeResponse::ack())
        })
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mocking the ClickHouseInserterRepository
    // which is complex due to the Inserter<T> internals.
    // Integration tests with real ClickHouse provide better coverage.
}
