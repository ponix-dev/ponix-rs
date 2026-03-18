---
title: Runner — Process Orchestration Framework
description: Concurrent process runner with coordinated shutdown and cleanup for long-running services.
crate: runner
category: module
related-files:
  - crates/runner/src/lib.rs
  - crates/runner/Cargo.toml
  - crates/ponix_all_in_one/src/main.rs
last-updated: 2026-03-18
---

# Runner — Process Orchestration Framework

The `runner` crate provides a lightweight framework for running multiple long-lived async processes concurrently with coordinated graceful shutdown. It exists because the ponix monolith needs to run several independent workers (gRPC server, CDC worker, analytics consumers, collaboration server, etc.) in a single binary while ensuring that if any one fails, everything shuts down cleanly and resources are released.

## Overview

The Runner is the top-level orchestrator in `ponix_all_in_one`. It has three responsibilities:

1. **Concurrent execution** — Spawn all registered app processes as independent tokio tasks running in parallel via a `JoinSet`.
2. **Coordinated shutdown** — Listen for OS signals (SIGTERM, SIGINT) or process failures, then cancel all remaining processes through a shared `CancellationToken`.
3. **Resource cleanup** — After all processes stop, run registered closer functions concurrently within a configurable timeout to release database connections, flush buffers, etc.

The crate has zero workspace dependencies — it depends only on `tokio`, `tokio-util`, `tracing`, and `anyhow` — making it reusable outside the ponix ecosystem.

## Key Concepts

- **`AppProcess`** — A boxed async closure that receives a `CancellationToken` and runs until completion or cancellation. This is the unit of work the Runner manages.
- **`NamedProcess`** — An `AppProcess` paired with a human-readable name used in structured log output (`process = "ponix_api"`).
- **`Closer`** — A boxed async closure for teardown logic (closing database pools, flushing NATS connections). All closers run concurrently after processes stop.
- **`Runner`** — The orchestrator struct. Built with a fluent API: `Runner::new().with_named_process(...).with_closer(...).run().await`.
- **`CancellationToken`** — From `tokio-util`. Shared across all processes; when cancelled, each process is expected to observe it via `tokio::select!` and exit gracefully. Can also be injected externally via `with_cancellation_token()` for testing or hierarchical cancellation.

## How It Works

### Registration Phase

Workers are added to the Runner using the builder pattern. Each crate in the workspace exposes an `into_runner_process()` method that returns an `AppProcess`. In `ponix_all_in_one`, this looks like:

```rust
let mut runner = Runner::new();
runner = runner.with_named_process("ponix_api", ponix_api.into_runner_process());
runner = runner.with_named_process("cdc_worker", cdc_worker.into_runner_process());
// ... more processes
runner = runner.with_closer(|| async { postgres_pool.close().await; Ok(()) });
runner = runner.with_closer_timeout(Duration::from_secs(10));
```

Some workers return multiple processes (e.g., `analytics_worker.into_runner_processes()` returns a `Vec<AppProcess>`) which are each registered separately.

### Execution Phase (`run()`)

1. **Spawn processes** — Each `NamedProcess` is spawned into a `JoinSet`. Every process receives a clone of the shared `CancellationToken`.
2. **Spawn signal handlers** — Separate tasks listen for `ctrl_c` (SIGINT) and `SIGTERM` (on Unix). Either signal cancels the token.
3. **Monitor the JoinSet** — The Runner polls `join_next()` in a loop. On a successful completion, it logs and continues. On an error or panic, it logs the failure, stores the error, cancels the token, and breaks out of the loop.
4. **Drain remaining tasks** — After the loop breaks (due to cancellation or all processes finishing), `join_set.shutdown().await` waits for all spawned tasks to complete.

### Cleanup Phase

5. **Run closers** — All closers are spawned concurrently in a second `JoinSet`, wrapped in `tokio::time::timeout` with the configured duration (default 10 seconds). Individual closer failures are logged but do not prevent other closers from running.
6. **Exit** — If any process returned an error, the binary exits with code 1. Otherwise, exit code 0.

### Design Decisions

- **Fail-fast on first error**: When any process fails, all others are cancelled immediately. This prevents the system from running in a degraded state where, for example, the CDC worker is down but the API is still accepting writes.
- **Closers are best-effort**: Closer failures are logged but don't change the exit code. A hung database connection shouldn't prevent the rest of cleanup from running.
- **`std::process::exit()` at the end**: The Runner calls `exit()` explicitly rather than returning. This ensures the process terminates even if background tokio tasks are still alive — a pragmatic choice for a service binary where lingering tasks after shutdown indicate a bug, not expected behavior.
- **No dependency on domain types**: The crate knows nothing about gRPC, NATS, or PostgreSQL. Each worker is responsible for its own cancellation-aware loop; the Runner just provides the coordination primitives.

## Related Documentation

- [CLAUDE.md](../../CLAUDE.md) — Project-level architecture overview including the Runner in context
