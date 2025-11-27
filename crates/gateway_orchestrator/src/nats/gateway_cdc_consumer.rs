use crate::domain::GatewayOrchestrationService;
use anyhow::{Context, Result};
use common::domain::GatewayCdcEvent;
use common::nats::{set_parent_from_headers, NatsClient};
use common::proto::parse_gateway_cdc_event;
use futures::StreamExt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info_span, Instrument};

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
        debug!("starting gateway_cdc_consumer");

        // Create durable consumer if not exists
        let consumer = self.create_or_get_consumer().await?;

        loop {
            tokio::select! {
                _ = ctx.cancelled() => {
                    debug!("gateway_cdc_consumer shutdown signal received");
                    break;
                }
                result = self.fetch_and_process_batch(&consumer) => {
                    if let Err(e) = result {
                        error!("error processing CDC batch: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }

        debug!("gateway_cdc_consumer stopped");
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
            .context("failed to create gateway CDC consumer")?;

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
            .context("failed to fetch messages")?;

        while let Some(message) = messages.next().await {
            let msg = match message {
                Ok(m) => m,
                Err(e) => {
                    error!("error receiving message: {}", e);
                    continue;
                }
            };

            match self.process_message(&msg).await {
                Ok(()) => {
                    if let Err(e) = msg.ack().await {
                        error!("failed to ack message: {}", e);
                    }
                }
                Err(e) => {
                    error!(
                        "failed to process CDC message from subject {}: {}",
                        msg.subject, e
                    );
                    if let Err(e) = msg
                        .ack_with(async_nats::jetstream::AckKind::Nak(None))
                        .await
                    {
                        error!("failed to nak message: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_message(&self, msg: &async_nats::jetstream::Message) -> Result<()> {
        let event = parse_gateway_cdc_event(&msg.subject, &msg.payload)?;

        // Create a span for processing this CDC event
        let headers = msg.headers.as_ref().cloned().unwrap_or_default();
        let span = info_span!(
            "process_gateway_cdc_event",
            gateway_id = %event.gateway_id(),
            subject = %msg.subject
        );

        async move {
            // Set the parent context from NATS headers on the current span
            // This connects this span to the trace that published the message
            set_parent_from_headers(&headers);

            debug!("processing CDC event for gateway: {}", event.gateway_id());

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
        .instrument(span)
        .await
    }
}
