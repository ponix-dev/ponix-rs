use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use crate::nats::{ConsumeRequest, ConsumeResponse};
use tower::{Layer, Service};
use tracing::{error, info, Instrument, Span};

/// Tower layer for logging single NATS message consumption
#[derive(Clone, Default)]
pub struct NatsConsumeLoggingLayer;

impl NatsConsumeLoggingLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for NatsConsumeLoggingLayer {
    type Service = NatsConsumeLoggingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        NatsConsumeLoggingService { inner: service }
    }
}

/// Service that logs single message consumption
#[derive(Clone)]
pub struct NatsConsumeLoggingService<S> {
    inner: S,
}

impl<S> Service<ConsumeRequest> for NatsConsumeLoggingService<S>
where
    S: Service<ConsumeRequest, Response = ConsumeResponse> + Clone + Send + 'static,
    S::Error: std::fmt::Display + Send,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: ConsumeRequest) -> Self::Future {
        let subject = req.subject.clone();
        let payload_size = req.payload.len();
        let start = Instant::now();
        let mut inner = self.inner.clone();

        let span = Span::current();

        Box::pin(
            async move {
                let result = inner.call(req).await;
                let duration = start.elapsed();

                match &result {
                    Ok(response) => {
                        let outcome = if response.is_ack() { "ack" } else { "nak" };
                        let duration_ms = duration.as_millis();

                        info!(
                            subject = %subject,
                            payload_bytes = payload_size,
                            outcome = %outcome,
                            duration_ms = %duration_ms,
                            "consumed from {subject} in {duration_ms}ms [{outcome}]"
                        );
                    }
                    Err(e) => {
                        let duration_ms = duration.as_millis();

                        error!(
                            subject = %subject,
                            payload_bytes = payload_size,
                            duration_ms = %duration_ms,
                            error = %e,
                            "failed to consume from {subject} in {duration_ms}ms: {e}"
                        );
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}
