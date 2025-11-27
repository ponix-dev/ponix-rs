use anyhow::Result;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;
use tracing::debug;

/// PostgreSQL client wrapper with connection pooling
#[derive(Clone)]
pub struct PostgresClient {
    pool: Pool,
}

impl PostgresClient {
    /// Creates a new PostgreSQL client with connection pooling
    ///
    /// # Arguments
    /// * `host` - Database host (e.g., "localhost")
    /// * `port` - Database port (e.g., 5432)
    /// * `database` - Database name
    /// * `username` - Database username
    /// * `password` - Database password
    /// * `max_pool_size` - Maximum number of connections in the pool
    pub fn new(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
        max_pool_size: usize,
    ) -> Result<Self> {
        let mut cfg = Config::new();
        cfg.host = Some(host.to_string());
        cfg.port = Some(port);
        cfg.dbname = Some(database.to_string());
        cfg.user = Some(username.to_string());
        cfg.password = Some(password.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;

        // Set pool size
        pool.resize(max_pool_size);

        Ok(Self { pool })
    }

    /// Pings the database to verify connectivity
    pub async fn ping(&self) -> Result<()> {
        let client = self.pool.get().await?;
        client.execute("SELECT 1", &[]).await?;
        debug!("postgreSQL connection successful");
        Ok(())
    }

    /// Gets a connection from the pool
    pub async fn get_connection(&self) -> Result<deadpool_postgres::Client> {
        Ok(self.pool.get().await?)
    }
}
