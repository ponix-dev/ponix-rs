mod client;
mod consumer;

#[cfg(feature = "processed-envelope")]
mod processed_envelope_processor;
#[cfg(feature = "processed-envelope")]
mod processed_envelope_producer;

pub use client::NatsClient;
pub use consumer::{BatchProcessor, NatsConsumer, ProcessingResult};

#[cfg(feature = "processed-envelope")]
pub use processed_envelope_processor::create_processed_envelope_processor;
#[cfg(feature = "processed-envelope")]
pub use processed_envelope_producer::ProcessedEnvelopeProducer;
