#[cfg(feature = "processed-envelope")]
use anyhow::{Context, Result};
#[cfg(feature = "processed-envelope")]
use async_nats::jetstream;
#[cfg(feature = "processed-envelope")]
use ponix_proto::envelope::v1::ProcessedEnvelope;
#[cfg(feature = "processed-envelope")]
use prost::Message;
#[cfg(feature = "processed-envelope")]
use tracing::{debug, info};

#[cfg(feature = "processed-envelope")]
pub struct ProcessedEnvelopeProducer {
    jetstream: jetstream::Context,
    base_subject: String,
}

#[cfg(feature = "processed-envelope")]
impl ProcessedEnvelopeProducer {
    pub fn new(jetstream: jetstream::Context, base_subject: String) -> Self {
        info!(
            "Created ProcessedEnvelopeProducer with base subject: {}",
            base_subject
        );
        Self {
            jetstream,
            base_subject,
        }
    }

    pub async fn publish(&self, envelope: &ProcessedEnvelope) -> Result<()> {
        // Serialize protobuf message
        let payload = envelope.encode_to_vec();

        // Build subject: {base_subject}.{end_device_id}
        let subject = format!("{}.{}", self.base_subject, envelope.end_device_id);

        debug!(
            subject = %subject,
            end_device_id = %envelope.end_device_id,
            size_bytes = payload.len(),
            "Publishing ProcessedEnvelope"
        );

        // Publish to JetStream
        let ack = self
            .jetstream
            .publish(subject.clone(), payload.into())
            .await
            .context("Failed to publish message to JetStream")?;

        // Await acknowledgment from JetStream
        ack.await
            .context("Failed to receive JetStream acknowledgment")?;

        debug!(
            subject = %subject,
            end_device_id = %envelope.end_device_id,
            "Successfully published and acknowledged"
        );

        Ok(())
    }
}
