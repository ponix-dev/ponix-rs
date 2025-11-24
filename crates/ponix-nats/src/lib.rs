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

#[cfg(feature = "raw-envelope")]
mod raw_envelope_conversions;
#[cfg(feature = "raw-envelope")]
mod raw_envelope_demo_producer;
#[cfg(feature = "raw-envelope")]
mod raw_envelope_processor;
#[cfg(feature = "raw-envelope")]
mod raw_envelope_producer;

pub use client::{NatsClient, NatsJetStreamConsumer, NatsJetStreamPublisher, NatsPullConsumer};
pub use consumer::{BatchProcessor, NatsConsumer, ProcessingResult};
pub use traits::{JetStreamConsumer, JetStreamPublisher, PullConsumer};

#[cfg(feature = "protobuf")]
pub use protobuf::{create_protobuf_processor, DecodedMessage, ProtobufHandler};

#[cfg(feature = "processed-envelope")]
pub use conversions::proto_to_domain_envelope;
#[cfg(feature = "processed-envelope")]
pub use processed_envelope_processor::create_domain_processor;
#[cfg(feature = "processed-envelope")]
pub use processed_envelope_producer::ProcessedEnvelopeProducer;

#[cfg(feature = "raw-envelope")]
pub use raw_envelope_conversions::{
    domain_to_proto as raw_envelope_domain_to_proto,
    proto_to_domain as raw_envelope_proto_to_domain,
};
#[cfg(feature = "raw-envelope")]
pub use raw_envelope_demo_producer::{
    run_raw_envelope_demo_producer, RawEnvelopeDemoProducerConfig,
};
#[cfg(feature = "raw-envelope")]
pub use raw_envelope_processor::create_domain_processor as create_raw_envelope_domain_processor;
#[cfg(feature = "raw-envelope")]
pub use raw_envelope_producer::RawEnvelopeProducer;
