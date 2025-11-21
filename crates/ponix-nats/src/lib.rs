mod client;
mod consumer;
mod traits;

#[cfg(feature = "protobuf")]
mod protobuf;

#[cfg(feature = "processed-envelope")]
mod conversions;
#[cfg(feature = "processed-envelope")]
mod processed_envelope_processor;
#[cfg(feature = "processed-envelope")]
mod processed_envelope_producer;
#[cfg(feature = "processed-envelope")]
mod demo_producer;

pub use client::{NatsClient, NatsJetStreamConsumer, NatsJetStreamPublisher, NatsPullConsumer};
pub use consumer::{BatchProcessor, NatsConsumer, ProcessingResult};
pub use traits::{JetStreamConsumer, JetStreamPublisher, PullConsumer};

#[cfg(feature = "protobuf")]
pub use protobuf::{create_protobuf_processor, DecodedMessage, ProtobufHandler};

#[cfg(feature = "processed-envelope")]
pub use conversions::proto_to_domain_envelope;
#[cfg(feature = "processed-envelope")]
pub use processed_envelope_processor::{
    create_domain_processor
};
#[cfg(feature = "processed-envelope")]
pub use processed_envelope_producer::ProcessedEnvelopeProducer;
#[cfg(feature = "processed-envelope")]
pub use demo_producer::{run_demo_producer, DemoProducerConfig};
