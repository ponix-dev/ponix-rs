use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use super::types::{PublishRequest, PublishResponse};
use tower::{Layer, Service};
use tracing::{debug, error, Instrument, Span};

/// Configuration for NATS logging middleware
#[derive(Clone, Debug, Default)]
pub struct NatsLoggingConfig {
    /// Whether to log successful publishes at debug level
    pub log_success: bool,
}

impl NatsLoggingConfig {
    pub fn new() -> Self {
        Self { log_success: true }
    }

    pub fn with_success_logging(mut self, enabled: bool) -> Self {
        self.log_success = enabled;
        self
    }
}

/// Tower layer for logging NATS publish operations
#[derive(Clone)]
pub struct NatsPublishLoggingLayer {
    config: NatsLoggingConfig,
}

impl NatsPublishLoggingLayer {
    pub fn new(config: NatsLoggingConfig) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for NatsPublishLoggingLayer {
    type Service = NatsPublishLoggingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        NatsPublishLoggingService {
            inner: service,
            config: self.config.clone(),
        }
    }
}

/// Service that logs NATS publish operations
#[derive(Clone)]
pub struct NatsPublishLoggingService<S> {
    inner: S,
    config: NatsLoggingConfig,
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
        let log_success = self.config.log_success;
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
                        if log_success {
                            debug!(
                                subject = %subject,
                                payload_size = payload_size,
                                duration_ms = %duration.as_millis(),
                                "published message to nats"
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            subject = %subject,
                            payload_size = payload_size,
                            duration_ms = %duration.as_millis(),
                            error = %e,
                            "failed to publish message to nats"
                        );
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}
