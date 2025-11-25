use crate::domain::GatewayOrchestrationService;
use anyhow::{Context, Result};
use common::{parse_gateway_cdc_event, GatewayCdcEvent, NatsClient};
use futures::StreamExt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub struct GatewayCdcConsumer {
    nats_client: Arc<NatsClient>,
    stream_name: String,
    consumer_name: String,
    filter_subject: String,
    orchestrator: Arc<GatewayOrchestrationService>,
}

impl GatewayCdcConsumer {
    pub fn new(
        nats_client: Arc<NatsClient>,
        stream_name: String,
        consumer_name: String,
        filter_subject: String,
        orchestrator: Arc<GatewayOrchestrationService>,
    ) -> Self {
        Self {
            nats_client,
            stream_name,
            consumer_name,
            filter_subject,
            orchestrator,
        }
    }

    /// Run the CDC consumer loop
    pub async fn run(&self, ctx: CancellationToken) -> Result<()> {
        info!("Starting GatewayCdcConsumer");

        // Create durable consumer if not exists
        let consumer = self.create_or_get_consumer().await?;

        loop {
            tokio::select! {
                _ = ctx.cancelled() => {
                    info!("GatewayCdcConsumer shutdown signal received");
                    break;
                }
                result = self.fetch_and_process_batch(&consumer) => {
                    if let Err(e) = result {
                        error!("Error processing CDC batch: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }

        info!("GatewayCdcConsumer stopped");
        Ok(())
    }

    async fn create_or_get_consumer(
        &self,
    ) -> Result<async_nats::jetstream::consumer::PullConsumer> {
        let js = self.nats_client.jetstream();

        let config = async_nats::jetstream::consumer::pull::Config {
            name: Some(self.consumer_name.clone()),
            durable_name: Some(self.consumer_name.clone()),
            filter_subject: self.filter_subject.clone(),
            ..Default::default()
        };

        let consumer = js
            .create_consumer_on_stream(config, &self.stream_name)
            .await
            .context("Failed to create gateway CDC consumer")?;

        Ok(consumer)
    }

    async fn fetch_and_process_batch(
        &self,
        consumer: &async_nats::jetstream::consumer::PullConsumer,
    ) -> Result<()> {
        let mut messages = consumer
            .fetch()
            .max_messages(10)
            .expires(tokio::time::Duration::from_secs(5))
            .messages()
            .await
            .context("Failed to fetch messages")?;

        while let Some(message) = messages.next().await {
            let msg = match message {
                Ok(m) => m,
                Err(e) => {
                    error!("Error receiving message: {}", e);
                    continue;
                }
            };

            match self.process_message(&msg).await {
                Ok(()) => {
                    if let Err(e) = msg.ack().await {
                        error!("Failed to ack message: {}", e);
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to process CDC message from subject {}: {}",
                        msg.subject, e
                    );
                    if let Err(e) = msg
                        .ack_with(async_nats::jetstream::AckKind::Nak(None))
                        .await
                    {
                        error!("Failed to nak message: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_message(&self, msg: &async_nats::jetstream::Message) -> Result<()> {
        let event = parse_gateway_cdc_event(&msg.subject, &msg.payload)?;

        info!("Processing CDC event for gateway: {}", event.gateway_id());

        match event {
            GatewayCdcEvent::Created { gateway } => {
                self.orchestrator
                    .handle_gateway_created(gateway)
                    .await
                    .map_err(|e| anyhow::anyhow!("Orchestrator error: {}", e))?;
            }
            GatewayCdcEvent::Updated { gateway: new } => {
                self.orchestrator
                    .handle_gateway_updated(new)
                    .await
                    .map_err(|e| anyhow::anyhow!("Orchestrator error: {}", e))?;
            }
            GatewayCdcEvent::Deleted { gateway_id } => {
                self.orchestrator
                    .handle_gateway_deleted(&gateway_id)
                    .await
                    .map_err(|e| anyhow::anyhow!("Orchestrator error: {}", e))?;
            }
        }

        Ok(())
    }
}
