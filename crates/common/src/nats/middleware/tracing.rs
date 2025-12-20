use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::nats::{inject_trace_context, PublishRequest, PublishResponse};
use tower::{Layer, Service};
use tracing::{field, info_span, Instrument, Span};

/// Configuration for NATS tracing middleware
#[derive(Clone, Debug)]
pub struct NatsTracingConfig {
    /// Service name for span attributes
    pub service_name: String,
    /// Whether to propagate trace context to consumers via headers.
    /// When true (default), the current trace context is injected into message headers.
    /// When false, each publish starts a fresh trace (useful for CDC/event-sourced systems
    /// where each event should be its own trace root).
    pub propagate_context: bool,
}

impl Default for NatsTracingConfig {
    fn default() -> Self {
        Self {
            service_name: String::new(),
            propagate_context: true,
        }
    }
}

impl NatsTracingConfig {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            propagate_context: true,
        }
    }

    /// Disable trace context propagation, making each publish a new trace root.
    ///
    /// This is useful for CDC workers or event-sourced systems where each
    /// database change should start a fresh trace rather than appending to
    /// a long-running worker trace.
    pub fn without_context_propagation(mut self) -> Self {
        self.propagate_context = false;
        self
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
        let subject = req.subject.clone();
        let payload_size = req.payload.len();
        let service_name = self.config.service_name.clone();
        let propagate_context = self.config.propagate_context;

        // Create span for this publish operation.
        // When propagate_context is false, create a root span (parent: None)
        // to start a fresh trace for each message.
        let span = if propagate_context {
            // Inject current trace context into headers for downstream consumers
            inject_trace_context(&mut req.headers);

            info_span!(
                target: "nats",
                "nats_publish",
                otel.name = "nats_publish",
                messaging.system = "nats",
                messaging.operation = "publish",
                messaging.destination.name = %subject,
                messaging.message.body.size = payload_size,
                service.name = %service_name,
                otel.status_code = field::Empty,
            )
        } else {
            // Create a root span with no parent - this starts a fresh trace
            let span = info_span!(
                target: "nats",
                parent: None,
                "nats_publish",
                otel.name = "nats_publish",
                messaging.system = "nats",
                messaging.operation = "publish",
                messaging.destination.name = %subject,
                messaging.message.body.size = payload_size,
                service.name = %service_name,
                otel.status_code = field::Empty,
            );

            // Inject this NEW span's trace context into headers
            // (must be done inside the span to capture its context)
            let _guard = span.enter();
            inject_trace_context(&mut req.headers);
            drop(_guard);

            span
        };

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
