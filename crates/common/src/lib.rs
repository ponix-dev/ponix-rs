mod clickhouse;
mod domain;
mod grpc;
mod nats;
mod postgres;
mod proto;

pub use clickhouse::*;
pub use domain::*;
pub use grpc::*;
pub use nats::*;
pub use postgres::*;
pub use proto::*;

// Re-export mocks when testing feature is enabled
#[cfg(any(test, feature = "testing"))]
pub use domain::MockDeviceRepository;
#[cfg(any(test, feature = "testing"))]
pub use domain::MockGatewayRepository;
#[cfg(any(test, feature = "testing"))]
pub use domain::MockOrganizationRepository;
#[cfg(any(test, feature = "testing"))]
pub use domain::MockProcessedEnvelopeProducer;
#[cfg(any(test, feature = "testing"))]
pub use domain::MockProcessedEnvelopeRepository;
#[cfg(any(test, feature = "testing"))]
pub use domain::MockRawEnvelopeProducer;
#[cfg(any(test, feature = "testing"))]
pub use nats::MockJetStreamConsumer;
#[cfg(any(test, feature = "testing"))]
pub use nats::MockJetStreamPublisher;
#[cfg(any(test, feature = "testing"))]
pub use nats::MockPullConsumer;
