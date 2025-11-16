use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseConfig {
    pub url: String,
    pub database: String,
    pub username: String,
    pub password: String,
    pub migrations_dir: String,
    pub goose_binary_path: String,
}

impl Default for ClickHouseConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8123".to_string(),
            database: "ponix".to_string(),
            username: "default".to_string(),
            password: "".to_string(),
            migrations_dir: "crates/ponix-clickhouse/migrations".to_string(),
            goose_binary_path: "goose".to_string(),
        }
    }
}
