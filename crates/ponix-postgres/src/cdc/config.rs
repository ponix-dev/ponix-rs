use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CdcConfig {
    // PostgreSQL settings
    pub pg_host: String,
    pub pg_port: u16,
    pub pg_database: String,
    pub pg_user: String,
    pub pg_password: String,

    // CDC settings
    pub publication_name: String,
    pub slot_name: String,

    // Batching
    pub batch_size: usize,
    pub batch_timeout_ms: u64,

    // Retry
    pub retry_delay_ms: u64,
    pub max_retry_attempts: u32,
}

impl CdcConfig {
    /// Creates a PostgreSQL connection string from the config
    pub fn connection_string(&self) -> String {
        format!(
            "host={} port={} dbname={} user={} password={}",
            self.pg_host, self.pg_port, self.pg_database, self.pg_user, self.pg_password
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_string() {
        let config = CdcConfig {
            pg_host: "localhost".into(),
            pg_port: 5432,
            pg_database: "testdb".into(),
            pg_user: "testuser".into(),
            pg_password: "testpass".into(),
            publication_name: "test_pub".into(),
            slot_name: "test_slot".into(),
            batch_size: 100,
            batch_timeout_ms: 5000,
            retry_delay_ms: 10000,
            max_retry_attempts: 5,
        };

        let conn_str = config.connection_string();
        assert!(conn_str.contains("host=localhost"));
        assert!(conn_str.contains("port=5432"));
        assert!(conn_str.contains("dbname=testdb"));
        assert!(conn_str.contains("user=testuser"));
        assert!(conn_str.contains("password=testpass"));
    }
}
