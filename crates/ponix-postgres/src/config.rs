use serde::{Deserialize, Serialize};

/// PostgreSQL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub max_pool_size: usize,
    pub migrations_dir: String,
    pub goose_binary_path: String,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "ponix".to_string(),
            username: "ponix".to_string(),
            password: "ponix".to_string(),
            max_pool_size: 10,
            migrations_dir: "crates/ponix-postgres/migrations".to_string(),
            goose_binary_path: "goose".to_string(),
        }
    }
}
