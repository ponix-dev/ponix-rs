use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use super::consumer_tracing::BatchRequest;
use crate::nats::consumer::ProcessingResult;
use tower::{Layer, Service};
use tracing::{debug, error, info, Instrument, Span};

/// Tower layer for logging NATS batch consumption
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

/// Service that logs batch consumption
#[derive(Clone)]
pub struct NatsConsumeLoggingService<S> {
    inner: S,
}

impl<S> Service<BatchRequest> for NatsConsumeLoggingService<S>
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
        let start = Instant::now();
        let mut inner = self.inner.clone();

        let span = Span::current();

        Box::pin(
            async move {
                let result = inner.call(req).await;
                let duration = start.elapsed();

                match &result {
                    Ok(processing_result) => {
                        if batch_size > 0 {
                            info!(
                                stream = %stream_name,
                                consumer = %consumer_name,
                                batch_size = batch_size,
                                ack_count = processing_result.ack.len(),
                                nak_count = processing_result.nak.len(),
                                duration_ms = %duration.as_millis(),
                                "processed message batch"
                            );
                        } else {
                            debug!(
                                stream = %stream_name,
                                consumer = %consumer_name,
                                "no messages in batch"
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            stream = %stream_name,
                            consumer = %consumer_name,
                            batch_size = batch_size,
                            duration_ms = %duration.as_millis(),
                            error = %e,
                            "batch processing error"
                        );
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}
