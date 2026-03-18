---
title: Goose Migration Runner
description: Thin Rust wrapper around the goose binary for running database migrations as a subprocess.
crate: goose
category: module
related-files:
  - crates/goose/src/lib.rs
  - crates/goose/Cargo.toml
  - crates/init_process/migrations/postgres/
  - crates/init_process/migrations/clickhouse/
  - crates/ponix_all_in_one/src/main.rs
last-updated: 2026-03-18
---

# Goose Migration Runner

The `goose` crate provides a Rust-native interface to the [goose](https://github.com/pressly/goose) CLI tool for running database schema migrations. Rather than embedding migration logic or reimplementing goose in Rust, it delegates to the external goose binary via subprocess execution. This keeps the migration tooling decoupled from the application and allows the same migration files to be used by both the application and operators running goose directly.

## Overview

The crate's sole responsibility is spawning the goose binary with the correct arguments for a given database. It is intentionally minimal -- a single struct (`MigrationRunner`) with three operations: run pending migrations, roll back the last migration, and check migration status. This design supports multiple database backends (PostgreSQL, ClickHouse, MySQL, SQLite) through goose's driver abstraction, which is important because Ponix uses both PostgreSQL and ClickHouse.

In the service lifecycle, migrations run during Phase 2 (PostgreSQL) and Phase 3 (ClickHouse) of `ponix_all_in_one` startup, before any repository or worker initialization begins. This guarantees the schema is up to date before the application accepts any traffic.

## Key Concepts

- **`MigrationRunner`** -- The only public type. Holds the goose binary path, migrations directory, database driver name, and DSN. Constructed once per database target.
- **Driver abstraction** -- The `driver` field (e.g., `"postgres"`, `"clickhouse"`) is passed directly to goose, making the runner database-agnostic. Ponix creates two separate `MigrationRunner` instances at startup, one per database.
- **DSN construction** -- The caller is responsible for building the driver-specific connection string. PostgreSQL uses `postgres://user:pass@host:port/db?sslmode=disable`; ClickHouse uses `clickhouse://user:pass@host:port/db`.
- **Subprocess execution** -- Uses `std::process::Command` (blocking I/O) rather than `tokio::process::Command`. The `async` signatures exist for API consistency with the rest of the codebase, but the actual goose invocation blocks the current thread. This is acceptable because migrations run once at startup before the async runtime is under load.
- **Migration files** -- SQL files live in `crates/init_process/migrations/postgres/` and `crates/init_process/migrations/clickhouse/`, following goose's `{timestamp}_{description}.sql` naming convention.

## How It Works

### Running Migrations

`run_migrations()` spawns `goose -dir {migrations_dir} {driver} {dsn} up`. Goose tracks which migrations have already been applied in a `goose_db_version` table within the target database, so this is idempotent -- only pending migrations execute.

If the goose process exits with a non-zero status, the runner captures both stdout and stderr and returns them in an `anyhow::Error`. On success, the stdout output (which lists applied migrations) is logged at `debug` level.

### Rollback and Status

`rollback_migration()` runs `goose ... down`, which reverts exactly one migration. `migration_status()` runs `goose ... status` and returns the raw stdout as a `String`. These are available for operational use but are not called during normal application startup.

### Error Handling

Two failure modes:
1. **Binary not found** -- `Command::new()` fails if the goose binary does not exist at the configured path.
2. **Migration failure** -- Goose exits non-zero (syntax error in SQL, connection refused, constraint violation, etc.). The runner captures stderr and stdout and wraps them in an error message.

In both cases, the error propagates up to `ponix_all_in_one`'s `main()`, which logs the error and exits. The application never starts with an outdated schema.

## Related Documentation

- [All-in-One Service Binary](ponix-all-in-one.md) — Where migrations are invoked during startup
