use config::{Config, ConfigError, Environment};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    // NATS configuration
    /// NATS server URL
    #[serde(default = "default_nats_url")]
    pub nats_url: String,

    /// NATS JetStream stream name
    #[serde(default = "default_processed_envelopes_stream")]
    pub processed_envelopes_stream: String,

    /// NATS subject pattern for consumer filter
    #[serde(default = "default_processed_envelopes_subject")]
    pub processed_envelopes_subject: String,

    /// NATS JetStream stream name for raw envelopes
    #[serde(default = "default_nats_raw_stream")]
    pub nats_raw_stream: String,

    /// NATS subject pattern for raw envelope consumer filter
    #[serde(default = "default_nats_raw_subject")]
    pub nats_raw_subject: String,

    /// NATS JetStream stream name for gateway CDC events
    #[serde(default = "default_nats_gateway_stream")]
    pub nats_gateway_stream: String,

    /// NATS subject pattern for gateway CDC events
    #[serde(default = "default_nats_gateway_subject")]
    pub nats_gateway_subject: String,

    /// NATS JetStream stream name for workspace CDC events
    #[serde(default = "default_nats_workspace_stream")]
    pub nats_workspace_stream: String,

    /// NATS JetStream stream name for data stream CDC events
    #[serde(default = "default_nats_data_streams_stream")]
    pub nats_data_streams_stream: String,

    /// NATS JetStream stream name for organization CDC events
    #[serde(default = "default_nats_organization_stream")]
    pub nats_organization_stream: String,

    /// NATS JetStream stream name for user CDC events
    #[serde(default = "default_nats_user_stream")]
    pub nats_user_stream: String,

    /// NATS JetStream stream name for data stream definition CDC events
    #[serde(default = "default_nats_data_stream_definitions_stream")]
    pub nats_data_stream_definitions_stream: String,

    /// NATS Object Store bucket name for document storage
    #[serde(default = "default_nats_object_store_bucket")]
    pub nats_object_store_bucket: String,

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

    // gRPC configuration
    /// gRPC server host
    #[serde(default = "default_grpc_host")]
    pub grpc_host: String,

    /// gRPC server port
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,

    /// Enable gRPC-Web support (default: true for browser access)
    #[serde(default = "default_grpc_web_enabled")]
    pub grpc_web_enabled: bool,

    /// CORS allowed origins (comma-separated list, "*" for all origins)
    #[serde(default = "default_grpc_cors_allowed_origins")]
    pub grpc_cors_allowed_origins: String,

    // JWT configuration
    /// JWT signing secret (required for production)
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,

    /// JWT token expiration in hours (default: 24)
    #[serde(default = "default_jwt_expiration_hours")]
    pub jwt_expiration_hours: u64,

    /// Refresh token expiration in days (default: 7)
    #[serde(default = "default_refresh_token_expiration_days")]
    pub refresh_token_expiration_days: u64,

    /// Use secure cookies (HTTPS only) - should be true in production
    #[serde(default = "default_secure_cookies")]
    pub secure_cookies: bool,

    // CDC configuration
    /// CDC entity name for gateway events
    #[serde(default = "default_cdc_gateway_entity_name")]
    pub cdc_gateway_entity_name: String,

    /// CDC table name for gateway events
    #[serde(default = "default_cdc_gateway_table_name")]
    pub cdc_gateway_table_name: String,

    /// CDC entity name for workspace events
    #[serde(default = "default_cdc_workspace_entity_name")]
    pub cdc_workspace_entity_name: String,

    /// CDC table name for workspace events
    #[serde(default = "default_cdc_workspace_table_name")]
    pub cdc_workspace_table_name: String,

    /// CDC entity name for data stream events
    #[serde(default = "default_cdc_data_stream_entity_name")]
    pub cdc_data_stream_entity_name: String,

    /// CDC table name for data stream events
    #[serde(default = "default_cdc_data_stream_table_name")]
    pub cdc_data_stream_table_name: String,

    /// CDC entity name for organization events
    #[serde(default = "default_cdc_organization_entity_name")]
    pub cdc_organization_entity_name: String,

    /// CDC table name for organization events
    #[serde(default = "default_cdc_organization_table_name")]
    pub cdc_organization_table_name: String,

    /// CDC entity name for user events
    #[serde(default = "default_cdc_user_entity_name")]
    pub cdc_user_entity_name: String,

    /// CDC table name for user events
    #[serde(default = "default_cdc_user_table_name")]
    pub cdc_user_table_name: String,

    /// CDC entity name for data stream definition events
    #[serde(default = "default_cdc_data_stream_definition_entity_name")]
    pub cdc_data_stream_definition_entity_name: String,

    /// CDC table name for data stream definition events
    #[serde(default = "default_cdc_data_stream_definition_table_name")]
    pub cdc_data_stream_definition_table_name: String,

    /// CDC publication name
    #[serde(default = "default_cdc_publication_name")]
    pub cdc_publication_name: String,

    /// CDC replication slot name
    #[serde(default = "default_cdc_slot_name")]
    pub cdc_slot_name: String,

    /// CDC batch size
    #[serde(default = "default_cdc_batch_size")]
    pub cdc_batch_size: usize,

    /// CDC batch timeout in milliseconds
    #[serde(default = "default_cdc_batch_timeout_ms")]
    pub cdc_batch_timeout_ms: u64,

    /// CDC retry delay in milliseconds
    #[serde(default = "default_cdc_retry_delay_ms")]
    pub cdc_retry_delay_ms: u64,

    /// CDC max retry attempts
    #[serde(default = "default_cdc_max_retry_attempts")]
    pub cdc_max_retry_attempts: u32,

    // Gateway Orchestrator configuration
    /// Gateway CDC consumer name
    #[serde(default = "default_gateway_consumer_name")]
    pub gateway_consumer_name: String,

    /// Gateway CDC filter subject pattern
    #[serde(default = "default_gateway_filter_subject")]
    pub gateway_filter_subject: String,

    /// Gateway deployer type (in_process)
    #[serde(default = "default_gateway_deployer_type")]
    pub gateway_deployer_type: String,

    // OpenTelemetry configuration
    /// OpenTelemetry OTLP endpoint (gRPC)
    #[serde(default = "default_otel_endpoint")]
    pub otel_endpoint: String,

    /// Enable OpenTelemetry export
    #[serde(default = "default_otel_enabled")]
    pub otel_enabled: bool,

    /// Service name for OpenTelemetry resource
    #[serde(default = "default_otel_service_name")]
    pub otel_service_name: String,

    // Telemetry configuration
    /// gRPC paths to ignore in logging (comma-separated)
    #[serde(default = "default_grpc_ignored_paths")]
    pub grpc_ignored_paths: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

// NATS defaults
fn default_nats_url() -> String {
    "nats://localhost:4222".to_string()
}

fn default_processed_envelopes_stream() -> String {
    "processed_envelopes".to_string()
}

fn default_processed_envelopes_subject() -> String {
    "processed_envelopes.>".to_string()
}

fn default_nats_raw_stream() -> String {
    "raw_envelopes".to_string()
}

fn default_nats_raw_subject() -> String {
    "raw_envelopes.>".to_string()
}

fn default_nats_gateway_stream() -> String {
    "gateways".to_string()
}

fn default_nats_gateway_subject() -> String {
    "gateways.>".to_string()
}

fn default_nats_workspace_stream() -> String {
    "workspaces".to_string()
}

fn default_nats_data_streams_stream() -> String {
    "data_streams".to_string()
}

fn default_nats_organization_stream() -> String {
    "organizations".to_string()
}

fn default_nats_user_stream() -> String {
    "users".to_string()
}

fn default_nats_data_stream_definitions_stream() -> String {
    "data_stream_definitions".to_string()
}

fn default_nats_object_store_bucket() -> String {
    "ponix-documents".to_string()
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

// gRPC defaults
fn default_grpc_host() -> String {
    "0.0.0.0".to_string()
}

fn default_grpc_port() -> u16 {
    50051
}

fn default_grpc_web_enabled() -> bool {
    true
}

fn default_grpc_cors_allowed_origins() -> String {
    "*".to_string()
}

// JWT defaults
fn default_jwt_secret() -> String {
    "change-me-in-production".to_string()
}

fn default_jwt_expiration_hours() -> u64 {
    24
}

fn default_refresh_token_expiration_days() -> u64 {
    7
}

fn default_secure_cookies() -> bool {
    false
}

// CDC defaults
fn default_cdc_gateway_entity_name() -> String {
    "gateways".to_string()
}

fn default_cdc_gateway_table_name() -> String {
    "gateways".to_string()
}

fn default_cdc_workspace_entity_name() -> String {
    "workspaces".to_string()
}

fn default_cdc_workspace_table_name() -> String {
    "workspaces".to_string()
}

fn default_cdc_data_stream_entity_name() -> String {
    "data_streams".to_string()
}

fn default_cdc_data_stream_table_name() -> String {
    "data_streams".to_string()
}

fn default_cdc_organization_entity_name() -> String {
    "organizations".to_string()
}

fn default_cdc_organization_table_name() -> String {
    "organizations".to_string()
}

fn default_cdc_user_entity_name() -> String {
    "users".to_string()
}

fn default_cdc_user_table_name() -> String {
    "users".to_string()
}

fn default_cdc_data_stream_definition_entity_name() -> String {
    "data_stream_definitions".to_string()
}

fn default_cdc_data_stream_definition_table_name() -> String {
    "data_stream_definitions".to_string()
}

fn default_cdc_publication_name() -> String {
    "ponix_cdc_publication".to_string()
}

fn default_cdc_slot_name() -> String {
    "ponix_cdc_slot".to_string()
}

fn default_cdc_batch_size() -> usize {
    100
}

fn default_cdc_batch_timeout_ms() -> u64 {
    5000
}

fn default_cdc_retry_delay_ms() -> u64 {
    10000
}

fn default_cdc_max_retry_attempts() -> u32 {
    5
}

// Gateway Orchestrator defaults
fn default_gateway_consumer_name() -> String {
    "gateway-orchestrator-consumer".to_string()
}

fn default_gateway_filter_subject() -> String {
    "gateways.>".to_string()
}

fn default_gateway_deployer_type() -> String {
    "in_process".to_string()
}

// OpenTelemetry defaults
fn default_otel_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_otel_enabled() -> bool {
    true
}

fn default_otel_service_name() -> String {
    "ponix-all-in-one".to_string()
}

// Telemetry defaults
fn default_grpc_ignored_paths() -> String {
    "/grpc.reflection.".to_string()
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
        // SAFETY: Test runs with mutex lock to prevent concurrent env access
        unsafe {
            std::env::remove_var("PONIX_LOG_LEVEL");
        }

        let config = ServiceConfig::from_env().unwrap();
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn test_custom_config() {
        let _lock = TEST_LOCK.lock().unwrap();

        // SAFETY: Test runs with mutex lock to prevent concurrent env access
        unsafe {
            std::env::set_var("PONIX_LOG_LEVEL", "debug");
        }

        let config = ServiceConfig::from_env().unwrap();
        assert_eq!(config.log_level, "debug");

        // Clean up
        // SAFETY: Test runs with mutex lock to prevent concurrent env access
        unsafe {
            std::env::remove_var("PONIX_LOG_LEVEL");
        }
    }
}
