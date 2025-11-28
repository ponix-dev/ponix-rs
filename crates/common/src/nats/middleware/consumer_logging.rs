use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use super::consumer_types::{ConsumeRequest, ConsumeResponse};
use tower::{Layer, Service};
use tracing::{debug, error, Instrument, Span};

/// Configuration for consume logging
#[derive(Clone, Debug)]
pub struct NatsConsumeLoggingConfig {
    /// Log level for successful processing (debug by default)
    pub log_success_level: LogLevel,
}

#[derive(Clone, Debug, Default)]
pub enum LogLevel {
    #[default]
    Debug,
    Info,
}

impl Default for NatsConsumeLoggingConfig {
    fn default() -> Self {
        Self {
            log_success_level: LogLevel::Debug,
        }
    }
}

impl NatsConsumeLoggingConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_info_level(mut self) -> Self {
        self.log_success_level = LogLevel::Info;
        self
    }
}

/// Tower layer for logging single NATS message consumption
#[derive(Clone, Default)]
pub struct NatsConsumeLoggingLayer {
    config: NatsConsumeLoggingConfig,
}

impl NatsConsumeLoggingLayer {
    pub fn new(config: NatsConsumeLoggingConfig) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for NatsConsumeLoggingLayer {
    type Service = NatsConsumeLoggingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        NatsConsumeLoggingService {
            inner: service,
            config: self.config.clone(),
        }
    }
}

/// Service that logs single message consumption
#[derive(Clone)]
pub struct NatsConsumeLoggingService<S> {
    inner: S,
    config: NatsConsumeLoggingConfig,
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
        let log_level = self.config.log_success_level.clone();

        let span = Span::current();

        Box::pin(
            async move {
                let result = inner.call(req).await;
                let duration = start.elapsed();

                match &result {
                    Ok(response) => {
                        let outcome = if response.is_ack() { "ack" } else { "nak" };

                        match log_level {
                            LogLevel::Debug => {
                                debug!(
                                    subject = %subject,
                                    payload_bytes = payload_size,
                                    outcome = %outcome,
                                    duration_ms = %duration.as_millis(),
                                    "processed message"
                                );
                            }
                            LogLevel::Info => {
                                tracing::info!(
                                    subject = %subject,
                                    payload_bytes = payload_size,
                                    outcome = %outcome,
                                    duration_ms = %duration.as_millis(),
                                    "processed message"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            subject = %subject,
                            payload_bytes = payload_size,
                            duration_ms = %duration.as_millis(),
                            error = %e,
                            "message processing error"
                        );
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}
