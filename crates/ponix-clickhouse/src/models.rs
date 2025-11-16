use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ProcessedEnvelopeRow {
    pub organization_id: String,
    pub end_device_id: String,
    // CRITICAL: This serde attribute is required when using JSON type alongside DateTime
    // Reference: https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_derive_simple.rs
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub occurred_at: DateTime<Utc>,
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub processed_at: DateTime<Utc>,
    // JSON type in ClickHouse, mapped to String in Rust
    pub data: String,
}
