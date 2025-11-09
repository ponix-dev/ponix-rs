use anyhow::{Context, Result};
use async_nats::jetstream::{self, consumer::PullConsumer, Message};
use futures::{future::BoxFuture, StreamExt};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Result of processing a batch of messages
/// Provides fine-grained control over which messages to acknowledge vs reject
#[derive(Debug)]
pub struct ProcessingResult {
    /// Messages that were successfully processed and should be acknowledged (Ack)
    pub ack: Vec<usize>,
    /// Messages that failed processing and should be rejected (Nak) with optional error details
    pub nak: Vec<(usize, Option<String>)>,
}

impl ProcessingResult {
    /// Create a result where all messages should be acknowledged
    pub fn ack_all(count: usize) -> Self {
        Self {
            ack: (0..count).collect(),
            nak: Vec::new(),
        }
    }

    /// Create a result where all messages should be rejected
    pub fn nak_all(count: usize, error: Option<String>) -> Self {
        Self {
            ack: Vec::new(),
            nak: (0..count).map(|i| (i, error.clone())).collect(),
        }
    }

    /// Create a result with specific ack/nak indices
    pub fn new(ack: Vec<usize>, nak: Vec<(usize, Option<String>)>) -> Self {
        Self { ack, nak }
    }
}

/// Type alias for the batch processor function
/// Takes a slice of raw NATS messages and returns a ProcessingResult
/// The processor is responsible for deserializing and processing the messages
pub type BatchProcessor =
    Box<dyn Fn(&[Message]) -> BoxFuture<'static, Result<ProcessingResult>> + Send + Sync>;

/// Generic NATS JetStream consumer that processes batches of messages
/// The consumer handles fetching messages, acknowledgments, and error handling
/// Message deserialization and business logic are delegated to the processor function
pub struct NatsConsumer {
    consumer: PullConsumer,
    batch_size: usize,
    max_wait: Duration,
    processor: BatchProcessor,
}

impl NatsConsumer {
    pub async fn new(
        jetstream: &jetstream::Context,
        stream_name: &str,
        consumer_name: &str,
        subject_filter: &str,
        batch_size: usize,
        max_wait_secs: u64,
        processor: BatchProcessor,
    ) -> Result<Self> {
        debug!(
            stream = stream_name,
            consumer = consumer_name,
            subject = subject_filter,
            "Creating JetStream consumer"
        );

        // Create or get existing durable consumer
        let consumer = jetstream
            .create_consumer_on_stream(
                jetstream::consumer::pull::Config {
                    name: Some(consumer_name.to_string()),
                    durable_name: Some(consumer_name.to_string()),
                    filter_subject: subject_filter.to_string(),
                    ack_policy: jetstream::consumer::AckPolicy::Explicit,
                    ..Default::default()
                },
                stream_name,
            )
            .await
            .context("Failed to create consumer")?;

        info!(
            stream = stream_name,
            consumer = consumer_name,
            "Consumer created successfully"
        );

        Ok(Self {
            consumer,
            batch_size,
            max_wait: Duration::from_secs(max_wait_secs),
            processor,
        })
    }

    pub async fn run(&self, ctx: CancellationToken) -> Result<()> {
        info!("Starting consumer loop");

        loop {
            tokio::select! {
                _ = ctx.cancelled() => {
                    info!("Received shutdown signal, stopping consumer");
                    break;
                }
                result = self.fetch_and_process_batch() => {
                    if let Err(e) = result {
                        error!(error = %e, "Error processing batch");
                        // Continue processing despite errors
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        info!("Consumer stopped gracefully");
        Ok(())
    }

    async fn fetch_and_process_batch(&self) -> Result<()> {
        debug!(
            batch_size = self.batch_size,
            max_wait_secs = self.max_wait.as_secs(),
            "Fetching message batch"
        );

        // Fetch batch of messages
        let mut messages = self
            .consumer
            .fetch()
            .max_messages(self.batch_size)
            .expires(self.max_wait)
            .messages()
            .await
            .context("Failed to fetch messages")?;

        let mut raw_messages = Vec::new();

        // Collect messages from stream
        while let Some(result) = messages.next().await {
            match result {
                Ok(msg) => {
                    raw_messages.push(msg);
                }
                Err(e) => {
                    warn!(error = %e, "Error receiving message from batch");
                }
            }
        }

        if raw_messages.is_empty() {
            debug!("No messages in batch");
            return Ok(());
        }

        debug!(message_count = raw_messages.len(), "Received message batch");

        // Process batch using the custom processor
        // The processor is responsible for deserialization and business logic
        let processing_result = match (self.processor)(&raw_messages).await {
            Ok(result) => result,
            Err(e) => {
                // If the processor returns an error, Nak all messages
                error!(error = %e, "Processor returned error, rejecting all messages");
                ProcessingResult::nak_all(raw_messages.len(), Some(e.to_string()))
            }
        };

        // Process acknowledgments
        let ack_count = processing_result.ack.len();
        for idx in processing_result.ack {
            if let Some(msg) = raw_messages.get(idx) {
                if let Err(e) = msg.ack().await {
                    error!(
                        error = %e,
                        message_index = idx,
                        "Failed to acknowledge message"
                    );
                }
            } else {
                warn!(
                    message_index = idx,
                    batch_size = raw_messages.len(),
                    "Invalid ack index in ProcessingResult"
                );
            }
        }

        if ack_count > 0 {
            debug!(ack_count, "Acknowledged messages");
        }

        // Process rejections
        let nak_count = processing_result.nak.len();
        for (idx, error_msg) in processing_result.nak {
            if let Some(msg) = raw_messages.get(idx) {
                if let Some(err) = error_msg {
                    error!(
                        message_index = idx,
                        subject = %msg.subject,
                        error = %err,
                        "Rejecting message due to processing error"
                    );
                } else {
                    warn!(
                        message_index = idx,
                        subject = %msg.subject,
                        "Rejecting message"
                    );
                }

                if let Err(e) = msg.ack_with(jetstream::AckKind::Nak(None)).await {
                    error!(
                        error = %e,
                        message_index = idx,
                        "Failed to reject message"
                    );
                }
            } else {
                warn!(
                    message_index = idx,
                    batch_size = raw_messages.len(),
                    "Invalid nak index in ProcessingResult"
                );
            }
        }

        if nak_count > 0 {
            debug!(nak_count, "Rejected messages for redelivery");
        }

        Ok(())
    }
}
