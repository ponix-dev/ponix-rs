use serde::{Deserialize, Serialize};
use config::{Config, ConfigError, Environment};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceConfig {
    /// Message to print (placeholder for future config)
    #[serde(default = "default_message")]
    pub message: String,

    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Sleep interval in seconds
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

fn default_message() -> String {
    "ponix-all-in-one service is running".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_interval() -> u64 {
    5
}

impl ServiceConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Config::builder()
            .add_source(Environment::with_prefix("PONIX"))
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to ensure tests run serially and don't interfere with each other
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_config() {
        let _lock = TEST_LOCK.lock().unwrap();

        // Clear any existing PONIX_ environment variables
        std::env::remove_var("PONIX_MESSAGE");
        std::env::remove_var("PONIX_LOG_LEVEL");
        std::env::remove_var("PONIX_INTERVAL_SECS");

        let config = ServiceConfig::from_env().unwrap();
        assert_eq!(config.message, "ponix-all-in-one service is running");
        assert_eq!(config.log_level, "info");
        assert_eq!(config.interval_secs, 5);
    }

    #[test]
    fn test_custom_config() {
        let _lock = TEST_LOCK.lock().unwrap();

        std::env::set_var("PONIX_MESSAGE", "Custom message");
        std::env::set_var("PONIX_LOG_LEVEL", "debug");
        std::env::set_var("PONIX_INTERVAL_SECS", "10");

        let config = ServiceConfig::from_env().unwrap();
        assert_eq!(config.message, "Custom message");
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.interval_secs, 10);

        // Clean up
        std::env::remove_var("PONIX_MESSAGE");
        std::env::remove_var("PONIX_LOG_LEVEL");
        std::env::remove_var("PONIX_INTERVAL_SECS");
    }
}
