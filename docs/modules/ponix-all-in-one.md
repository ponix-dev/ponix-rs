---
title: All-in-One Service Binary
description: Monolith binary that orchestrates all Ponix workers, infrastructure clients, and domain services into a single deployable unit.
crate: ponix_all_in_one
category: module
related-files:
  - crates/ponix_all_in_one/src/main.rs
  - crates/ponix_all_in_one/src/config.rs
  - crates/ponix_all_in_one/Cargo.toml
  - crates/runner/src/lib.rs
last-updated: 2026-03-18
---

# All-in-One Service Binary

`ponix_all_in_one` is the main entry point for the Ponix platform, assembling every crate in the workspace into a single deployable monolith. It exists so the system can ship as one binary today while each worker crate remains independently extractable into its own microservice later. All wiring -- database connections, message broker streams, domain services, and worker processes -- lives here and nowhere else.

## Overview

This crate has two responsibilities: **bootstrap** (connect to infrastructure, run migrations, construct the dependency graph) and **orchestrate** (register every worker as a concurrent process with the `Runner` framework and manage graceful shutdown). It owns no business logic of its own; it is pure composition.

Key responsibilities:
- Load configuration from environment variables (`PONIX_` prefix)
- Initialize telemetry (OpenTelemetry traces and logs)
- Run PostgreSQL and ClickHouse migrations before accepting traffic
- Create and share infrastructure clients (Postgres, ClickHouse, NATS)
- Construct all PostgreSQL repository implementations and wrap them in `Arc`
- Build domain services with their dependencies (repositories, authorization, auth tokens)
- Ensure all required NATS JetStream streams exist (10 streams total)
- Register six worker subsystems as named `Runner` processes
- Coordinate shutdown: cancel processes, drain NATS, flush telemetry

## Key Concepts

- **`ServiceConfig`** -- Single struct deserialized from `PONIX_`-prefixed env vars via the `config` crate. Every tunable in the system flows through here. All fields have sensible defaults for local development.
- **`PostgresRepositories`** -- Private struct that bundles all nine `Arc<Postgres*Repository>` instances, constructed once and shared across services. This is the single point where repository implementations are chosen.
- **`Runner`** -- From the `ponix-runner` crate. Accepts named processes (`AppProcess` closures) and closers. Runs all processes concurrently; if any fails, all are cancelled via `CancellationToken`.
- **`into_runner_process()` / `into_runner_processes()`** -- Convention each worker crate implements. Some workers (analytics, snapshotter) return multiple processes because they run both a consumer and a background loop.
- **`ensure_nats_streams()`** -- Idempotently creates all JetStream streams the system needs. This means the service is self-provisioning -- no external stream setup required.

## How It Works

### Initialization Phases

The `main()` function executes a strict sequential bootstrap before entering concurrent mode:

1. **Configuration** -- `ServiceConfig::from_env()` deserializes all `PONIX_*` environment variables. Failure here prints to stderr and exits (tracing is not yet initialized).

2. **Telemetry** -- `init_telemetry()` sets up `tracing-subscriber` with optional OTLP export. From this point forward, structured logging is available.

3. **Shared Infrastructure** -- Runs in order:
   - PostgreSQL migrations (via `goose` binary subprocess)
   - PostgreSQL client creation (deadpool, 5 max connections)
   - Nine repository instances wrapped in `Arc`
   - ClickHouse migrations (via `goose` binary subprocess)
   - ClickHouse client creation + ping
   - NATS connection (with configurable startup timeout, default 30s)
   - 10 JetStream streams ensured

4. **Authorization** -- Casbin authorization service initialized from PostgreSQL policy adapter.

5. **Domain Services** -- Eight services constructed: `DataStreamService`, `DataStreamDefinitionService`, `OrganizationService`, `GatewayService`, `UserService`, `WorkspaceService`, `DocumentService`, plus auth token/password services (JWT + Argon2).

6. **Worker Construction** -- Six subsystems built with their specific configs:
   - `PonixApi` -- gRPC server (port 50051) with gRPC-Web and CORS support
   - `GatewayOrchestrator` -- CDC consumer that maintains in-memory gateway state
   - `CdcWorker` -- PostgreSQL logical replication consumer, publishes to NATS
   - `AnalyticsWorker` -- Raw envelope CEL transformation + ClickHouse ingestion
   - `CollaborationServer` -- WebSocket server (port 50052) for Yrs document editing
   - `DocumentSnapshotter` -- NATS consumer + compaction worker for persisting Yrs state

7. **Runner Assembly** -- Each worker registers as a named process. Workers that return multiple processes are registered with indexed names.

8. **Run** -- `runner.run().await` starts all processes concurrently and blocks until shutdown signal or failure.

### Shutdown Sequence

When a SIGTERM/SIGINT arrives or any process returns an error:
1. `Runner` cancels the shared `CancellationToken`, signaling all processes
2. Each worker's `tokio::select!` branch fires, allowing graceful drain
3. The closer runs: orchestrator token cancelled, NATS drained, telemetry flushed
4. If the closer exceeds 10 seconds, it is forcefully terminated

### Design Decisions

**Why a monolith?** Each worker crate has no dependency on `ponix_all_in_one` -- the dependency arrows all point inward. This means any crate can be given its own `main.rs` and deployed independently when the time comes. The monolith avoids operational complexity during early development.

**Why migrations at startup?** Running migrations in the application binary ensures the schema is always consistent with the code version. The `goose` binary is invoked as a subprocess, keeping migration tooling decoupled from Rust code.

**Why explicit stream creation?** `ensure_nats_streams()` makes the service self-provisioning. A fresh NATS instance needs no manual setup -- the service creates its own streams idempotently on every boot.

## Configuration

All configuration is loaded from environment variables with the `PONIX_` prefix via `ServiceConfig::from_env()`. The full schema is defined in `crates/ponix_all_in_one/src/config.rs`.

Key variable groups:

| Group | Prefix | Examples |
|-------|--------|----------|
| Logging | `PONIX_LOG_LEVEL` | `info`, `debug`, `trace` |
| PostgreSQL | `PONIX_POSTGRES_*` | `HOST`, `PORT`, `DATABASE`, `USERNAME`, `PASSWORD` |
| ClickHouse | `PONIX_CLICKHOUSE_*` | `URL`, `NATIVE_URL`, `DATABASE` |
| NATS | `PONIX_NATS_*` | `URL`, `BATCH_SIZE`, `BATCH_WAIT_SECS` |
| gRPC | `PONIX_GRPC_*` | `HOST`, `PORT`, `WEB_ENABLED`, `CORS_ALLOWED_ORIGINS` |
| Collaboration | `PONIX_COLLAB_*` | `HOST`, `PORT`, `CORS_ALLOWED_ORIGINS` |
| CDC | `PONIX_CDC_*` | `PUBLICATION_NAME`, `SLOT_NAME`, `BATCH_SIZE` |
| Snapshotter | `PONIX_SNAPSHOTTER_*` | `COMPACTION_INTERVAL_SECS`, `IDLE_EVICTION_SECS` |
| Telemetry | `PONIX_OTEL_*` | `ENABLED`, `ENDPOINT`, `SERVICE_NAME` |

See the full environment variable reference in `CLAUDE.md` under "Environment Configuration."

## Related Documentation

- [Runner](runner.md) — Process orchestration framework
- [Common](common.md) — Shared infrastructure, domain types, and repository traits
- [Analytics Worker](analytics-worker.md) — Envelope processing pipeline
- [CDC Worker](cdc-worker.md) — PostgreSQL Change Data Capture
- [Collaboration Server](collaboration-server.md) — Real-time Yrs document collaboration
