use common::domain::{DomainError, DomainResult, GatewayConfig};
use std::sync::Arc;

use crate::domain::GatewayRunner;

/// Type alias for a function that creates a gateway runner.
pub type GatewayRunnerConstructor = Box<dyn Fn() -> Arc<dyn GatewayRunner> + Send + Sync>;

/// Factory for creating gateway runners based on gateway configuration.
///
/// This factory uses a registry pattern where runner constructors are registered
/// at initialization time, decoupling the domain layer from specific implementations.
///
/// # Example
/// ```ignore
/// use gateway_orchestrator::domain::{GatewayRunnerFactory, GatewayRunner};
/// use gateway_orchestrator::mqtt::EmqxGatewayRunner;
///
/// let mut factory = GatewayRunnerFactory::new();
/// factory.register_emqx(|| Arc::new(EmqxGatewayRunner::new()));
///
/// let runner = factory.create_runner(&gateway.gateway_config)?;
/// ```
pub struct GatewayRunnerFactory {
    emqx_constructor: Option<GatewayRunnerConstructor>,
    // Future gateway types can be added here:
    // ttn_constructor: Option<GatewayRunnerConstructor>,
    // chirpstack_constructor: Option<GatewayRunnerConstructor>,
}

impl std::fmt::Debug for GatewayRunnerFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayRunnerFactory")
            .field("emqx_registered", &self.emqx_constructor.is_some())
            .finish()
    }
}

impl Default for GatewayRunnerFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayRunnerFactory {
    /// Create a new empty factory with no runners registered.
    pub fn new() -> Self {
        Self {
            emqx_constructor: None,
        }
    }

    /// Register a constructor for EMQX gateway runners.
    ///
    /// # Arguments
    /// * `constructor` - A function that creates an EMQX gateway runner
    pub fn register_emqx<F>(&mut self, constructor: F)
    where
        F: Fn() -> Arc<dyn GatewayRunner> + Send + Sync + 'static,
    {
        self.emqx_constructor = Some(Box::new(constructor));
    }

    /// Create a gateway runner for the given gateway configuration.
    ///
    /// # Arguments
    /// * `config` - The gateway configuration determining which runner to create
    ///
    /// # Returns
    /// * `Ok(Arc<dyn GatewayRunner>)` - The appropriate runner for the gateway type
    /// * `Err(DomainError)` - If the gateway type is not supported or not registered
    pub fn create_runner(&self, config: &GatewayConfig) -> DomainResult<Arc<dyn GatewayRunner>> {
        match config {
            GatewayConfig::Emqx(_) => self
                .emqx_constructor
                .as_ref()
                .map(|constructor| constructor())
                .ok_or_else(|| {
                    DomainError::InvalidGatewayConfig(
                        "EMQX gateway runner not registered".to_string(),
                    )
                }),
        }
    }

    /// Check if a gateway configuration is supported (has a registered constructor).
    ///
    /// # Arguments
    /// * `config` - The gateway configuration to check
    ///
    /// # Returns
    /// `true` if the gateway type has a registered constructor, `false` otherwise
    pub fn is_supported(&self, config: &GatewayConfig) -> bool {
        match config {
            GatewayConfig::Emqx(_) => self.emqx_constructor.is_some(),
        }
    }

    /// Get the gateway type name from a configuration.
    ///
    /// # Arguments
    /// * `config` - The gateway configuration
    ///
    /// # Returns
    /// The gateway type name (e.g., "EMQX")
    pub fn gateway_type_name(&self, config: &GatewayConfig) -> &'static str {
        match config {
            GatewayConfig::Emqx(_) => "EMQX",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::GatewayRunner;
    use async_trait::async_trait;
    use common::domain::{EmqxGatewayConfig, Gateway, RawEnvelopeProducer};
    use tokio_util::sync::CancellationToken;

    // Mock runner for testing
    struct MockGatewayRunner;

    #[async_trait]
    impl GatewayRunner for MockGatewayRunner {
        async fn run(
            &self,
            _gateway: Gateway,
            _config: crate::domain::GatewayOrchestrationServiceConfig,
            _process_token: CancellationToken,
            _shutdown_token: CancellationToken,
            _raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
        ) {
        }

        fn gateway_type(&self) -> &'static str {
            "MOCK_EMQX"
        }
    }

    #[test]
    fn test_create_emqx_runner_when_registered() {
        let mut factory = GatewayRunnerFactory::new();
        factory.register_emqx(|| Arc::new(MockGatewayRunner));

        let config = GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://localhost:1883".to_string(),
        });

        let runner = factory.create_runner(&config);
        assert!(runner.is_ok());

        let runner = runner.unwrap();
        assert_eq!(runner.gateway_type(), "MOCK_EMQX");
    }

    #[test]
    fn test_create_emqx_runner_when_not_registered() {
        let factory = GatewayRunnerFactory::new();

        let config = GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://localhost:1883".to_string(),
        });

        let runner = factory.create_runner(&config);
        assert!(runner.is_err());
    }

    #[test]
    fn test_is_supported_when_registered() {
        let mut factory = GatewayRunnerFactory::new();
        factory.register_emqx(|| Arc::new(MockGatewayRunner));

        let config = GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://localhost:1883".to_string(),
        });

        assert!(factory.is_supported(&config));
    }

    #[test]
    fn test_is_supported_when_not_registered() {
        let factory = GatewayRunnerFactory::new();

        let config = GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://localhost:1883".to_string(),
        });

        assert!(!factory.is_supported(&config));
    }

    #[test]
    fn test_gateway_type_name() {
        let factory = GatewayRunnerFactory::new();
        let config = GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://localhost:1883".to_string(),
        });

        assert_eq!(factory.gateway_type_name(&config), "EMQX");
    }
}
