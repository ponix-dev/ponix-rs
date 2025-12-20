use crate::nats::{ConsumeRequest, ConsumeResponse, JetStreamConsumer, PullConsumer};
use anyhow::{Context, Result};
use async_nats::jetstream::{self};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tower::Service;
use tracing::{debug, error, info, warn};

/// A NATS consumer that processes messages through a Tower service stack.
///
/// Unlike the batch-based `NatsConsumer`, this processes messages one at a time,
/// allowing for proper Tower middleware composition (tracing, logging, etc.).
///
/// Messages are converted to owned `ConsumeRequest` types before being passed
/// to the service, which returns a `ConsumeResponse` indicating ack/nak.
pub struct TowerConsumer<S> {
    consumer: Box<dyn PullConsumer>,
    stream_name: String,
    consumer_name: String,
    batch_size: usize,
    max_wait: Duration,
    service: S,
}

impl<S> TowerConsumer<S>
where
    S: Service<ConsumeRequest, Response = ConsumeResponse, Error = anyhow::Error>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    /// Create a new Tower-based consumer
    pub async fn new(
        jetstream: Arc<dyn JetStreamConsumer>,
        stream_name: &str,
        consumer_name: &str,
        subject_filter: &str,
        batch_size: usize,
        max_wait_secs: u64,
        service: S,
    ) -> Result<Self> {
        debug!(
            stream = %stream_name,
            consumer = %consumer_name,
            filter_subject = %subject_filter,
            "creating tower nats consumer"
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
            .context("failed to create consumer")?;

        debug!(
            stream = %stream_name,
            consumer = %consumer_name,
            "tower nats consumer created successfully"
        );

        Ok(Self {
            consumer,
            stream_name: stream_name.to_string(),
            consumer_name: consumer_name.to_string(),
            batch_size,
            max_wait: Duration::from_secs(max_wait_secs),
            service,
        })
    }

    /// Run the consumer loop until cancellation
    pub async fn run(mut self, ctx: CancellationToken) -> Result<()> {
        debug!(
            stream = %self.stream_name,
            consumer = %self.consumer_name,
            "starting tower nats consumer"
        );

        loop {
            tokio::select! {
                _ = ctx.cancelled() => {
                    info!(
                        stream = %self.stream_name,
                        consumer = %self.consumer_name,
                        "received shutdown signal, stopping consumer"
                    );
                    break;
                }
                result = self.fetch_and_process_batch() => {
                    if let Err(e) = result {
                        error!(
                            stream = %self.stream_name,
                            consumer = %self.consumer_name,
                            error = %e,
                            "error processing batch"
                        );
                        // Continue processing despite errors
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        debug!(
            stream = %self.stream_name,
            consumer = %self.consumer_name,
            "consumer stopped gracefully"
        );
        Ok(())
    }

    async fn fetch_and_process_batch(&mut self) -> Result<()> {
        debug!(
            batch_size = self.batch_size,
            max_wait_secs = self.max_wait.as_secs(),
            "fetching message batch"
        );

        // Fetch messages using the trait method
        let raw_messages = self
            .consumer
            .fetch_messages(self.batch_size, self.max_wait)
            .await?;

        if raw_messages.is_empty() {
            debug!("no messages in batch");
            return Ok(());
        }

        debug!(message_count = raw_messages.len(), "received message batch");

        // Process each message individually through the Tower service
        for msg in &raw_messages {
            // Convert NATS message to owned ConsumeRequest
            let request = ConsumeRequest::new(
                msg.subject.to_string(),
                Bytes::copy_from_slice(&msg.payload),
                msg.headers.clone(),
            );

            // Call the Tower service
            let response = match self.service.call(request).await {
                Ok(resp) => resp,
                Err(e) => {
                    error!(
                        subject = %msg.subject,
                        error = %e,
                        "service error processing message"
                    );
                    ConsumeResponse::nak(e.to_string())
                }
            };

            // Handle ack/nak based on response
            match response {
                ConsumeResponse::Ack => {
                    if let Err(e) = msg.ack().await {
                        error!(
                            subject = %msg.subject,
                            error = %e,
                            "failed to acknowledge message"
                        );
                    }
                }
                ConsumeResponse::Nak(reason) => {
                    if let Some(ref r) = reason {
                        warn!(
                            subject = %msg.subject,
                            reason = %r,
                            "rejecting message"
                        );
                    } else {
                        warn!(
                            subject = %msg.subject,
                            "rejecting message"
                        );
                    }

                    if let Err(e) = msg.ack_with(jetstream::AckKind::Nak(None)).await {
                        error!(
                            subject = %msg.subject,
                            error = %e,
                            "failed to reject message"
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nats::traits::{MockJetStreamConsumer, MockPullConsumer};
    use futures::future::BoxFuture;
    use mockall::predicate::*;
    use std::task::{Context, Poll};

    /// Simple test service that acks everything
    #[derive(Clone)]
    struct AckAllService;

    impl Service<ConsumeRequest> for AckAllService {
        type Response = ConsumeResponse;
        type Error = anyhow::Error;
        type Future = BoxFuture<'static, Result<ConsumeResponse, anyhow::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: ConsumeRequest) -> Self::Future {
            Box::pin(async move { Ok(ConsumeResponse::Ack) })
        }
    }

    #[tokio::test]
    async fn test_tower_consumer_creation_success() {
        let mut mock_jetstream = MockJetStreamConsumer::new();

        mock_jetstream
            .expect_create_consumer()
            .withf(
                |config: &jetstream::consumer::pull::Config, stream_name: &str| {
                    config.durable_name.as_ref().unwrap() == "test-consumer"
                        && stream_name == "test-stream"
                },
            )
            .times(1)
            .returning(|_, _| Ok(Box::new(MockPullConsumer::new())));

        let result = TowerConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            AckAllService,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tower_consumer_creation_failure() {
        let mut mock_jetstream = MockJetStreamConsumer::new();

        mock_jetstream
            .expect_create_consumer()
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("Failed to create consumer")));

        let result = TowerConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            AckAllService,
        )
        .await;

        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("failed to create consumer"));
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

        let mut consumer = TowerConsumer::new(
            Arc::new(mock_jetstream),
            "test-stream",
            "test-consumer",
            "test.subject",
            10,
            5,
            AckAllService,
        )
        .await
        .unwrap();

        let result = consumer.fetch_and_process_batch().await;
        assert!(result.is_ok());
    }
}
