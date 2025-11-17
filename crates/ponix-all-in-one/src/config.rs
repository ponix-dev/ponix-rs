use config::{Config, ConfigError, Environment};
use serde::{Deserialize, Serialize};

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

    // NATS configuration
    /// NATS server URL
    #[serde(default = "default_nats_url")]
    pub nats_url: String,

    /// NATS JetStream stream name
    #[serde(default = "default_nats_stream")]
    pub nats_stream: String,

    /// NATS subject pattern for consumer filter
    #[serde(default = "default_nats_subject")]
    pub nats_subject: String,

    /// Batch size for consumer
    #[serde(default = "default_nats_batch_size")]
    pub nats_batch_size: usize,

    /// Max wait time for batches in seconds
    #[serde(default = "default_nats_batch_wait_secs")]
    pub nats_batch_wait_secs: u64,

    /// Startup timeout for initialization operations in seconds
    #[serde(default = "default_startup_timeout_secs")]
    pub startup_timeout_secs: u64,

    // ClickHouse configuration
    /// ClickHouse HTTP URL (for client connections)
    #[serde(default = "default_clickhouse_url")]
    pub clickhouse_url: String,

    /// ClickHouse native TCP URL (for migrations with goose)
    #[serde(default = "default_clickhouse_native_url")]
    pub clickhouse_native_url: String,

    /// ClickHouse database name
    #[serde(default = "default_clickhouse_database")]
    pub clickhouse_database: String,

    /// ClickHouse username
    #[serde(default = "default_clickhouse_username")]
    pub clickhouse_username: String,

    /// ClickHouse password
    #[serde(default = "default_clickhouse_password")]
    pub clickhouse_password: String,

    /// Path to migrations directory
    #[serde(default = "default_clickhouse_migrations_dir")]
    pub clickhouse_migrations_dir: String,

    /// Path to goose binary
    #[serde(default = "default_clickhouse_goose_binary_path")]
    pub clickhouse_goose_binary_path: String,

    // PostgreSQL configuration
    /// PostgreSQL host
    #[serde(default = "default_postgres_host")]
    pub postgres_host: String,

    /// PostgreSQL port
    #[serde(default = "default_postgres_port")]
    pub postgres_port: u16,

    /// PostgreSQL database name
    #[serde(default = "default_postgres_database")]
    pub postgres_database: String,

    /// PostgreSQL username
    #[serde(default = "default_postgres_username")]
    pub postgres_username: String,

    /// PostgreSQL password
    #[serde(default = "default_postgres_password")]
    pub postgres_password: String,

    /// Path to PostgreSQL migrations directory
    #[serde(default = "default_postgres_migrations_dir")]
    pub postgres_migrations_dir: String,

    /// Path to goose binary for PostgreSQL (usually same as ClickHouse)
    #[serde(default = "default_postgres_goose_binary_path")]
    pub postgres_goose_binary_path: String,
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

// NATS defaults
fn default_nats_url() -> String {
    "nats://localhost:4222".to_string()
}

fn default_nats_stream() -> String {
    "processed_envelopes".to_string()
}

fn default_nats_subject() -> String {
    "processed_envelopes.>".to_string()
}

fn default_nats_batch_size() -> usize {
    30
}

fn default_nats_batch_wait_secs() -> u64 {
    5
}

fn default_startup_timeout_secs() -> u64 {
    30
}

// ClickHouse defaults
fn default_clickhouse_url() -> String {
    "http://localhost:8123".to_string()
}

fn default_clickhouse_native_url() -> String {
    "localhost:9000".to_string()
}

fn default_clickhouse_database() -> String {
    "ponix".to_string()
}

fn default_clickhouse_username() -> String {
    "ponix".to_string()
}

fn default_clickhouse_password() -> String {
    "ponix".to_string()
}

fn default_clickhouse_migrations_dir() -> String {
    "/home/ponix/migrations/clickhouse".to_string()
}

fn default_clickhouse_goose_binary_path() -> String {
    "goose".to_string()
}

// PostgreSQL defaults
fn default_postgres_host() -> String {
    "localhost".to_string()
}

fn default_postgres_port() -> u16 {
    5432
}

fn default_postgres_database() -> String {
    "ponix".to_string()
}

fn default_postgres_username() -> String {
    "ponix".to_string()
}

fn default_postgres_password() -> String {
    "ponix".to_string()
}

fn default_postgres_migrations_dir() -> String {
    "/home/ponix/migrations/postgres".to_string()
}

fn default_postgres_goose_binary_path() -> String {
    "goose".to_string()
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
