use crate::domain::GatewayOrchestrationService;
use common::domain::GatewayCdcEvent;
use common::nats::{ConsumeRequest, ConsumeResponse};
use common::proto::parse_gateway_cdc_event;
use futures::future::BoxFuture;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;
use tracing::{debug, error};

/// Tower service for processing individual Gateway CDC messages.
///
/// This service:
/// 1. Parses the protobuf message into a GatewayCdcEvent
/// 2. Delegates to the GatewayOrchestrationService for handling
/// 3. Returns Ack on success, Nak on failure
#[derive(Clone)]
pub struct GatewayCdcService {
    orchestrator: Arc<GatewayOrchestrationService>,
}

impl GatewayCdcService {
    pub fn new(orchestrator: Arc<GatewayOrchestrationService>) -> Self {
        Self { orchestrator }
    }
}

impl Service<ConsumeRequest> for GatewayCdcService {
    type Response = ConsumeResponse;
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ConsumeRequest) -> Self::Future {
        let orchestrator = Arc::clone(&self.orchestrator);
        let subject = req.subject.clone();
        let payload = req.payload.clone();

        Box::pin(async move {
            // Parse the CDC event from the message
            let event = match parse_gateway_cdc_event(&subject, &payload) {
                Ok(e) => e,
                Err(e) => {
                    error!(
                        subject = %subject,
                        error = %e,
                        "failed to parse gateway CDC event"
                    );
                    return Ok(ConsumeResponse::Nak(Some(format!(
                        "parse error: {}",
                        e
                    ))));
                }
            };

            // Extract gateway_id for logging before consuming the event
            let gateway_id = event.gateway_id().to_string();

            debug!(
                gateway_id = %gateway_id,
                subject = %subject,
                "processing gateway CDC event"
            );

            // Handle the event based on type
            let result = match event {
                GatewayCdcEvent::Created { gateway } => {
                    orchestrator.handle_gateway_created(gateway).await
                }
                GatewayCdcEvent::Updated { gateway: new } => {
                    orchestrator.handle_gateway_updated(new).await
                }
                GatewayCdcEvent::Deleted { gateway_id: id } => {
                    orchestrator.handle_gateway_deleted(&id).await
                }
            };

            match result {
                Ok(()) => {
                    debug!(
                        gateway_id = %gateway_id,
                        "gateway CDC event processed successfully"
                    );
                    Ok(ConsumeResponse::Ack)
                }
                Err(e) => {
                    error!(
                        gateway_id = %gateway_id,
                        error = %e,
                        "failed to process gateway CDC event"
                    );
                    Ok(ConsumeResponse::Nak(Some(format!(
                        "orchestrator error: {}",
                        e
                    ))))
                }
            }
        })
    }
}
