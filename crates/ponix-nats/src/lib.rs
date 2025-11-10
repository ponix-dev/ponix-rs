mod client;
mod consumer;
mod traits;

#[cfg(feature = "processed-envelope")]
mod processed_envelope_processor;
#[cfg(feature = "processed-envelope")]
mod processed_envelope_producer;

pub use client::{NatsClient, NatsJetStreamConsumer, NatsJetStreamPublisher, NatsPullConsumer};
pub use consumer::{BatchProcessor, NatsConsumer, ProcessingResult};
pub use traits::{JetStreamConsumer, JetStreamPublisher, PullConsumer};

#[cfg(feature = "processed-envelope")]
pub use processed_envelope_processor::create_processed_envelope_processor;
#[cfg(feature = "processed-envelope")]
pub use processed_envelope_producer::ProcessedEnvelopeProducer;
