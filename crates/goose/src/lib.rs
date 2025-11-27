use anyhow::{bail, Result};
use std::process::Command;
use tracing::debug;

/// Generic migration runner for goose-compatible databases.
///
/// This runner executes goose migrations by spawning the goose binary as a subprocess.
/// It supports any database that goose supports (PostgreSQL, MySQL, SQLite, ClickHouse, etc.)
/// by accepting a generic DSN string.
pub struct MigrationRunner {
    /// Path to the goose binary (e.g., "goose" if in PATH, or absolute path)
    goose_binary_path: String,

    /// Directory containing SQL migration files
    migrations_dir: String,

    /// Database driver name (e.g., "postgres", "clickhouse", "mysql", "sqlite3")
    driver: String,

    /// Database connection string (DSN) - format depends on the driver
    dsn: String,
}

impl MigrationRunner {
    /// Creates a new MigrationRunner
    ///
    /// # Arguments
    /// * `goose_binary_path` - Path to goose binary (e.g., "goose" or "/usr/local/bin/goose")
    /// * `migrations_dir` - Directory containing migration SQL files
    /// * `driver` - Database driver name (e.g., "postgres", "clickhouse")
    /// * `dsn` - Database connection string in driver-specific format
    ///
    /// # Example
    /// ```
    /// let runner = goose::MigrationRunner::new(
    ///     "goose".to_string(),
    ///     "migrations/".to_string(),
    ///     "postgres".to_string(),
    ///     "postgres://user:pass@localhost:5432/dbname?sslmode=disable".to_string(),
    /// );
    /// ```
    pub fn new(
        goose_binary_path: String,
        migrations_dir: String,
        driver: String,
        dsn: String,
    ) -> Self {
        Self {
            goose_binary_path,
            migrations_dir,
            driver,
            dsn,
        }
    }

    /// Runs all pending migrations
    ///
    /// Executes `goose -dir {migrations_dir} {driver} {dsn} up`
    ///
    /// # Errors
    /// Returns an error if:
    /// - The goose binary is not found
    /// - Migration execution fails
    /// - Database connection fails
    pub async fn run_migrations(&self) -> Result<()> {
        debug!("running migrations from directory: {}", self.migrations_dir);

        let output = Command::new(&self.goose_binary_path)
            .arg("-dir")
            .arg(&self.migrations_dir)
            .arg(&self.driver)
            .arg(&self.dsn)
            .arg("up")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!("Migration failed.\nstdout: {}\nstderr: {}", stdout, stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        debug!("migrations completed successfully:\n{}", stdout);

        Ok(())
    }

    /// Rolls back the most recent migration
    ///
    /// Executes `goose -dir {migrations_dir} {driver} {dsn} down`
    pub async fn rollback_migration(&self) -> Result<()> {
        debug!("rolling back most recent migration");

        let output = Command::new(&self.goose_binary_path)
            .arg("-dir")
            .arg(&self.migrations_dir)
            .arg(&self.driver)
            .arg(&self.dsn)
            .arg("down")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!("Rollback failed.\nstdout: {}\nstderr: {}", stdout, stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        debug!("rollback completed successfully:\n{}", stdout);

        Ok(())
    }

    /// Gets the current migration status
    ///
    /// Executes `goose -dir {migrations_dir} {driver} {dsn} status`
    pub async fn migration_status(&self) -> Result<String> {
        let output = Command::new(&self.goose_binary_path)
            .arg("-dir")
            .arg(&self.migrations_dir)
            .arg(&self.driver)
            .arg(&self.dsn)
            .arg("status")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to get migration status: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_runner_creation() {
        let runner = MigrationRunner::new(
            "goose".to_string(),
            "migrations/".to_string(),
            "postgres".to_string(),
            "postgres://localhost/test".to_string(),
        );

        assert_eq!(runner.goose_binary_path, "goose");
        assert_eq!(runner.driver, "postgres");
    }
}
