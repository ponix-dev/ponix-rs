use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayOrchestrationServiceConfig {
    /// Delay before retrying failed process start (default: 10 seconds)
    pub retry_delay_secs: u64,

    /// Maximum number of retry attempts (default: 3)
    pub max_retry_attempts: u32,
}

impl Default for GatewayOrchestrationServiceConfig {
    fn default() -> Self {
        Self {
            retry_delay_secs: 10,
            max_retry_attempts: 3,
        }
    }
}

impl GatewayOrchestrationServiceConfig {
    pub fn retry_delay(&self) -> Duration {
        Duration::from_secs(self.retry_delay_secs)
    }
}

// Hard-coded print interval - this is temporary and will be replaced with actual MQTT logic
pub const PRINT_INTERVAL_SECS: u64 = 5;
