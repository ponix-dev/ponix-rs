use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::consumer_types::{ConsumeRequest, ConsumeResponse};
use crate::nats::trace_context::set_parent_from_headers;
use async_nats::jetstream::Message;
use async_nats::HeaderMap;
use tower::{Layer, Service};
use tracing::{info_span, Instrument};

/// Configuration for consume tracing
#[derive(Clone, Debug)]
pub struct NatsConsumeTracingConfig {
    /// Operation name for the span
    pub operation_name: String,
}

impl NatsConsumeTracingConfig {
    pub fn new(operation_name: impl Into<String>) -> Self {
        Self {
            operation_name: operation_name.into(),
        }
    }
}

impl Default for NatsConsumeTracingConfig {
    fn default() -> Self {
        Self {
            operation_name: "nats_consume".to_string(),
        }
    }
}

/// Tower layer for tracing single NATS message consumption
#[derive(Clone, Default)]
pub struct NatsConsumeTracingLayer {
    config: NatsConsumeTracingConfig,
}

impl NatsConsumeTracingLayer {
    pub fn new(config: NatsConsumeTracingConfig) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for NatsConsumeTracingLayer {
    type Service = NatsConsumeTracingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        NatsConsumeTracingService {
            inner: service,
            config: self.config.clone(),
        }
    }
}

/// Service that adds tracing to single message consumption
#[derive(Clone)]
pub struct NatsConsumeTracingService<S> {
    inner: S,
    config: NatsConsumeTracingConfig,
}

impl<S> Service<ConsumeRequest> for NatsConsumeTracingService<S>
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
        let headers = req.headers.clone();
        let operation_name = self.config.operation_name.clone();

        // Create span for message processing
        let span = info_span!(
            target: "nats",
            "nats_consume",
            otel.name = %operation_name,
            messaging.system = "nats",
            messaging.operation = "receive",
            messaging.destination.name = %subject,
        );

        // Set parent context from headers if available
        if let Some(ref hdrs) = headers {
            set_parent_from_headers(hdrs);
        }

        let mut inner = self.inner.clone();

        Box::pin(
            async move {
                let result = inner.call(req).await;

                match &result {
                    Ok(response) => {
                        if response.is_ack() {
                            tracing::debug!("message processed successfully");
                        } else {
                            tracing::warn!("message processing failed, will nak");
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "message processing error");
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}

/// Helper to set trace context for individual message processing.
///
/// Call this at the start of processing each message when not using
/// the Tower middleware stack (e.g., in the GatewayCdcConsumer).
pub fn enter_message_trace_context(msg: &Message) {
    if let Some(headers) = &msg.headers {
        set_parent_from_headers(headers);
    }
}

/// Helper to set trace context from a HeaderMap directly.
pub fn enter_trace_context_from_headers(headers: &HeaderMap) {
    set_parent_from_headers(headers);
}
