use anyhow::{Context, Result};
use async_nats::jetstream::{self, Message};
use crate::traits::{JetStreamConsumer, PullConsumer};
use futures::future::BoxFuture;
use std::sync::Arc;
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
    consumer: Box<dyn PullConsumer>,
    batch_size: usize,
    max_wait: Duration,
    processor: BatchProcessor,
}

impl NatsConsumer {
    pub async fn new(
        jetstream: Arc<dyn JetStreamConsumer>,
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

        let config = jetstream::consumer::pull::Config {
            name: Some(consumer_name.to_string()),
            durable_name: Some(consumer_name.to_string()),
            filter_subject: subject_filter.to_string(),
            ack_policy: jetstream::consumer::AckPolicy::Explicit,
            ..Default::default()
        };

        let consumer = jetstream
            .create_consumer(config, stream_name)
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

        // Fetch messages using the trait method
        let raw_messages = self
            .consumer
            .fetch_messages(self.batch_size, self.max_wait)
            .await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{MockJetStreamConsumer, MockPullConsumer};
    use mockall::predicate::*;

    fn create_processor_ack_all() -> BatchProcessor {
        Box::new(|msgs| {
            let count = msgs.len();
            Box::pin(async move { Ok(ProcessingResult::ack_all(count)) })
        })
    }

    #[tokio::test]
    async fn test_consumer_creation_success() {
        let mut mock_jetstream = MockJetStreamConsumer::new();

        // Set up expectations
        mock_jetstream
            .expect_create_consumer()
            .withf(|config: &jetstream::consumer::pull::Config, stream_name: &str| {
                config.durable_name.as_ref().unwrap() == "test-consumer"
                    && stream_name == "test-stream"
            })
            .times(1)
            .returning(|_, _| Ok(Box::new(MockPullConsumer::new())));

        let processor = create_processor_ack_all();

        let result = NatsConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            processor,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_consumer_creation_failure() {
        let mut mock_jetstream = MockJetStreamConsumer::new();

        mock_jetstream
            .expect_create_consumer()
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("Failed to create consumer")));

        let processor = create_processor_ack_all();

        let result = NatsConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            processor,
        )
        .await;

        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Failed to create consumer"));
    }

    #[tokio::test]
    async fn test_fetch_and_process_empty_batch() {
        let mut mock_jetstream = MockJetStreamConsumer::new();

        mock_jetstream
            .expect_create_consumer()
            .times(1)
            .returning(|_, _| {
                let mut mock = MockPullConsumer::new();
                mock.expect_fetch_messages()
                    .times(1)
                    .returning(|_, _| Ok(vec![]));
                Ok(Box::new(mock))
            });

        let processor = create_processor_ack_all();

        let consumer = NatsConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            processor,
        )
        .await
        .unwrap();

        let result = consumer.fetch_and_process_batch().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_processing_result_ack_all() {
        let result = ProcessingResult::ack_all(5);
        assert_eq!(result.ack.len(), 5);
        assert_eq!(result.nak.len(), 0);
        assert_eq!(result.ack, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_processing_result_nak_all() {
        let result = ProcessingResult::nak_all(3, Some("error".to_string()));
        assert_eq!(result.ack.len(), 0);
        assert_eq!(result.nak.len(), 3);
        for (idx, (i, msg)) in result.nak.iter().enumerate() {
            assert_eq!(*i, idx);
            assert_eq!(msg.as_ref().unwrap(), "error");
        }
    }

    #[tokio::test]
    async fn test_processing_result_partial() {
        let result = ProcessingResult::new(
            vec![0, 2],
            vec![(1, Some("error".to_string())), (3, None)],
        );
        assert_eq!(result.ack, vec![0, 2]);
        assert_eq!(result.nak.len(), 2);
        assert_eq!(result.nak[0].0, 1);
        assert_eq!(result.nak[1].0, 3);
    }

    // Note: Graceful shutdown test is omitted here as it requires more complex
    // async mock behavior. This will be addressed in Phase 6 with a proper
    // Message mocking strategy. The graceful shutdown logic is tested via
    // integration tests with a real NATS server.
}
