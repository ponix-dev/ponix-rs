use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::types::{PublishRequest, PublishResponse};
use super::{NatsPublishLoggingLayer, NatsPublishTracingLayer, NatsTracingConfig};
use crate::nats::JetStreamPublisher;
use anyhow::Result;
use tower::{Service, ServiceBuilder};

/// Inner service that performs the actual NATS publish
#[derive(Clone)]
pub struct NatsPublishService {
    publisher: Arc<dyn JetStreamPublisher>,
}

impl NatsPublishService {
    pub fn new(publisher: Arc<dyn JetStreamPublisher>) -> Self {
        Self { publisher }
    }
}

impl Service<PublishRequest> for NatsPublishService {
    type Response = PublishResponse;
    type Error = anyhow::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: PublishRequest) -> Self::Future {
        let publisher = Arc::clone(&self.publisher);
        let subject = req.subject.clone();

        Box::pin(async move {
            publisher
                .publish_with_headers(subject.clone(), req.headers, req.payload)
                .await?;
            Ok(PublishResponse { subject })
        })
    }
}

/// Builder for creating a layered NATS publisher service
pub struct NatsPublisherBuilder {
    publisher: Arc<dyn JetStreamPublisher>,
    tracing_config: Option<NatsTracingConfig>,
    with_logging: bool,
}

impl NatsPublisherBuilder {
    pub fn new(publisher: Arc<dyn JetStreamPublisher>) -> Self {
        Self {
            publisher,
            tracing_config: None,
            with_logging: false,
        }
    }

    pub fn with_tracing(mut self, config: NatsTracingConfig) -> Self {
        self.tracing_config = Some(config);
        self
    }

    pub fn with_logging(mut self) -> Self {
        self.with_logging = true;
        self
    }

    /// Build the layered publisher service
    /// Layer order (outermost first): Tracing -> Logging -> Publish
    pub fn build(self) -> LayeredPublisher<NatsPublishService> {
        let inner = NatsPublishService::new(self.publisher);

        // Build the service stack with optional layers
        // We need to build conditionally based on which layers are configured
        match (self.tracing_config, self.with_logging) {
            (Some(tracing_config), true) => {
                // Both layers
                let svc = ServiceBuilder::new()
                    .layer(NatsPublishTracingLayer::new(tracing_config))
                    .layer(NatsPublishLoggingLayer::new())
                    .service(inner);
                LayeredPublisher::Both(svc)
            }
            (Some(tracing_config), false) => {
                // Only tracing
                let svc = ServiceBuilder::new()
                    .layer(NatsPublishTracingLayer::new(tracing_config))
                    .service(inner);
                LayeredPublisher::TracingOnly(svc)
            }
            (None, true) => {
                // Only logging
                let svc = ServiceBuilder::new()
                    .layer(NatsPublishLoggingLayer::new())
                    .service(inner);
                LayeredPublisher::LoggingOnly(svc)
            }
            (None, false) => {
                // No layers
                LayeredPublisher::None(inner)
            }
        }
    }
}

/// Enum to hold different layer combinations
/// This allows the builder to return a concrete type that implements Service
#[derive(Clone)]
pub enum LayeredPublisher<S> {
    Both(super::NatsPublishTracingService<super::NatsPublishLoggingService<NatsPublishService>>),
    TracingOnly(super::NatsPublishTracingService<NatsPublishService>),
    LoggingOnly(super::NatsPublishLoggingService<NatsPublishService>),
    None(S),
}

impl<S> Service<PublishRequest> for LayeredPublisher<S>
where
    S: Service<PublishRequest, Response = PublishResponse, Error = anyhow::Error>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = PublishResponse;
    type Error = anyhow::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self {
            LayeredPublisher::Both(svc) => svc.poll_ready(cx),
            LayeredPublisher::TracingOnly(svc) => svc.poll_ready(cx),
            LayeredPublisher::LoggingOnly(svc) => svc.poll_ready(cx),
            LayeredPublisher::None(svc) => svc.poll_ready(cx),
        }
    }

    fn call(&mut self, req: PublishRequest) -> Self::Future {
        match self {
            LayeredPublisher::Both(svc) => svc.call(req),
            LayeredPublisher::TracingOnly(svc) => svc.call(req),
            LayeredPublisher::LoggingOnly(svc) => svc.call(req),
            LayeredPublisher::None(svc) => Box::pin(svc.call(req)),
        }
    }
}
