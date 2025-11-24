use crate::gateway::Gateway;

/// Domain representation of a gateway CDC event
#[derive(Debug, Clone)]
pub enum GatewayCdcEvent {
    Created { gateway: Gateway },
    Updated {  gateway: Gateway },
    Deleted { gateway_id: String },
}

impl GatewayCdcEvent {
    pub fn gateway_id(&self) -> &str {
        match self {
            GatewayCdcEvent::Created { gateway } => &gateway.gateway_id,
            GatewayCdcEvent::Updated { gateway, .. } => &gateway.gateway_id,
            GatewayCdcEvent::Deleted { gateway_id } => gateway_id,
        }
    }
}
