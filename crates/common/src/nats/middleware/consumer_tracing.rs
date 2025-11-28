use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::nats::consumer::ProcessingResult;
use crate::nats::trace_context::set_parent_from_headers;
use async_nats::jetstream::Message;
use tower::{Layer, Service};
use tracing::{info_span, Instrument};

/// Request type for batch processing
#[derive(Debug)]
pub struct BatchRequest {
    pub messages: Vec<Message>,
    pub stream_name: String,
    pub consumer_name: String,
}

/// Tower layer for tracing NATS batch consumption
#[derive(Clone, Default)]
pub struct NatsConsumeTracingLayer;

impl NatsConsumeTracingLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for NatsConsumeTracingLayer {
    type Service = NatsConsumeTracingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        NatsConsumeTracingService { inner: service }
    }
}

/// Service that adds tracing to batch consumption
#[derive(Clone)]
pub struct NatsConsumeTracingService<S> {
    inner: S,
}

impl<S> Service<BatchRequest> for NatsConsumeTracingService<S>
where
    S: Service<BatchRequest, Response = ProcessingResult> + Clone + Send + 'static,
    S::Error: std::fmt::Display + Send,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: BatchRequest) -> Self::Future {
        let batch_size = req.messages.len();
        let stream_name = req.stream_name.clone();
        let consumer_name = req.consumer_name.clone();

        // Create span for batch processing
        let span = info_span!(
            target: "nats",
            "nats_consume_batch",
            otel.name = "nats_consume_batch",
            messaging.system = "nats",
            messaging.operation = "receive",
            messaging.batch.message_count = batch_size,
            messaging.destination.name = %stream_name,
            messaging.consumer.name = %consumer_name,
        );

        // For each message, we'll set the parent context when processing
        // The inner service will call set_parent_from_headers for per-message spans

        let mut inner = self.inner.clone();

        Box::pin(
            async move {
                let result = inner.call(req).await;

                match &result {
                    Ok(processing_result) => {
                        tracing::debug!(
                            ack_count = processing_result.ack.len(),
                            nak_count = processing_result.nak.len(),
                            "batch processing complete"
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "batch processing failed");
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}

/// Helper to set trace context for individual message processing
/// Call this at the start of processing each message in a batch
pub fn enter_message_trace_context(msg: &Message) {
    if let Some(headers) = &msg.headers {
        set_parent_from_headers(headers);
    }
}
