use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use super::types::{PublishRequest, PublishResponse};
use tower::{Layer, Service};
use tracing::{error, info, Instrument, Span};

/// Tower layer for logging NATS publish operations
#[derive(Clone, Default)]
pub struct NatsPublishLoggingLayer;

impl NatsPublishLoggingLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for NatsPublishLoggingLayer {
    type Service = NatsPublishLoggingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        NatsPublishLoggingService { inner: service }
    }
}

/// Service that logs NATS publish operations
#[derive(Clone)]
pub struct NatsPublishLoggingService<S> {
    inner: S,
}

impl<S> Service<PublishRequest> for NatsPublishLoggingService<S>
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

    fn call(&mut self, req: PublishRequest) -> Self::Future {
        let subject = req.subject.clone();
        let payload_size = req.payload.len();
        let start = Instant::now();
        let mut inner = self.inner.clone();

        // Capture current span for log correlation
        let span = Span::current();

        Box::pin(
            async move {
                let result = inner.call(req).await;
                let duration = start.elapsed();

                match &result {
                    Ok(_) => {
                        let duration_ms = duration.as_millis();

                        info!(
                            subject = %subject,
                            payload_bytes = payload_size,
                            duration_ms = %duration_ms,
                            "published to {subject} in {duration_ms}ms"
                        );
                    }
                    Err(e) => {
                        let duration_ms = duration.as_millis();

                        error!(
                            subject = %subject,
                            payload_bytes = payload_size,
                            duration_ms = %duration_ms,
                            error = %e,
                            "failed to publish to {subject} in {duration_ms}ms: {e}"
                        );
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}
