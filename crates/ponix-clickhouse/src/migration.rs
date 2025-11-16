use anyhow::{Context, Result};
use std::process::Command;
use tracing::info;

pub struct MigrationRunner {
    goose_binary_path: String,
    migrations_dir: String,
    clickhouse_url: String,
    database: String,
    user: String,
    password: String,
}

impl MigrationRunner {
    pub fn new(
        goose_binary_path: String,
        migrations_dir: String,
        clickhouse_url: String,
        database: String,
        user: String,
        password: String,
    ) -> Self {
        Self {
            goose_binary_path,
            migrations_dir,
            clickhouse_url,
            database,
            user,
            password,
        }
    }

    pub async fn run_migrations(&self) -> Result<()> {
        // Build connection string for goose with JSON type support
        // CRITICAL: Include allow_experimental_json_type in DSN for migrations
        let dsn = format!(
            "clickhouse://{}:{}@{}/{}?allow_experimental_json_type=1",
            self.user, self.password, self.clickhouse_url, self.database
        );

        info!("Running database migrations from {}", self.migrations_dir);

        let output = Command::new(&self.goose_binary_path)
            .args(["-dir", &self.migrations_dir, "clickhouse", &dsn, "up"])
            .output()
            .context("Failed to execute goose command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!("Migration failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("Migrations completed successfully: {}", stdout);

        Ok(())
    }
}
