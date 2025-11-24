/// Gateway configuration types
/// Mirrors the protobuf Gateway config structure but as a domain type
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayConfig {
    /// EMQX gateway configuration
    Emqx(EmqxGatewayConfig),
}

/// EMQX gateway configuration
#[derive(Debug, Clone, PartialEq)]
pub struct EmqxGatewayConfig {
    pub broker_url: String,
}
