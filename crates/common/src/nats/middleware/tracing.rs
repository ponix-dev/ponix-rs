use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::types::{PublishRequest, PublishResponse};
use crate::nats::trace_context::inject_trace_context;
use tower::{Layer, Service};
use tracing::{field, info_span, Instrument, Span};

/// Configuration for NATS tracing middleware
#[derive(Clone, Debug, Default)]
pub struct NatsTracingConfig {
    /// Service name for span attributes
    pub service_name: String,
}

impl NatsTracingConfig {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }
}

/// Tower layer for OpenTelemetry tracing of NATS publish operations
#[derive(Clone)]
pub struct NatsPublishTracingLayer {
    config: NatsTracingConfig,
}

impl NatsPublishTracingLayer {
    pub fn new(config: NatsTracingConfig) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for NatsPublishTracingLayer {
    type Service = NatsPublishTracingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        NatsPublishTracingService {
            inner: service,
            config: self.config.clone(),
        }
    }
}

/// Service that adds OpenTelemetry tracing to NATS publish operations
#[derive(Clone)]
pub struct NatsPublishTracingService<S> {
    inner: S,
    config: NatsTracingConfig,
}

impl<S> Service<PublishRequest> for NatsPublishTracingService<S>
where
    S: Service<PublishRequest, Response = PublishResponse> + Clone + Send + 'static,
    S::Error: std::fmt::Display + Send,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: PublishRequest) -> Self::Future {
        // Inject trace context into headers
        inject_trace_context(&mut req.headers);

        let subject = req.subject.clone();
        let payload_size = req.payload.len();
        let service_name = self.config.service_name.clone();

        // Create span for this publish operation
        let span = info_span!(
            target: "nats",
            "nats_publish",
            otel.name = "nats_publish",
            messaging.system = "nats",
            messaging.operation = "publish",
            messaging.destination.name = %subject,
            messaging.message.body.size = payload_size,
            service.name = %service_name,
            otel.status_code = field::Empty,
        );

        let mut inner = self.inner.clone();

        Box::pin(
            async move {
                let result = inner.call(req).await;

                match &result {
                    Ok(_) => {
                        Span::current().record("otel.status_code", "OK");
                    }
                    Err(e) => {
                        Span::current().record("otel.status_code", "ERROR");
                        tracing::error!(error = %e, "nats publish failed");
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}
