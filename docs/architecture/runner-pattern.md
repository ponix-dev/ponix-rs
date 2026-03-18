# Runner Pattern for Process Orchestration

## Overview

The runner pattern is the core concurrency framework that orchestrates all long-running processes in ponix-rs. It lives in the `ponix-runner` crate (`crates/runner/`) and provides concurrent execution, graceful shutdown via OS signals, and coordinated cleanup for the entire application.

The system currently runs as a monolith (`ponix_all_in_one`) but is designed so that each worker crate can be extracted into a standalone microservice with its own `Runner` instance.

## Core Types

The runner crate defines two key type aliases:

```rust
// A process function: receives a CancellationToken, returns a future
pub type AppProcess = Box<
    dyn FnOnce(CancellationToken) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send,
>;

// A cleanup function: no inputs, returns a future
pub type Closer =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>> + Send>;
```

The `Runner` struct holds a list of `NamedProcess` entries (name + `AppProcess`), a list of `Closer` functions, a configurable closer timeout (default 10 seconds), and a `CancellationToken`.

## Lifecycle

When `runner.run()` is called, the following sequence executes:

```
1. Log startup with process count
2. Spawn all AppProcess functions concurrently via JoinSet
3. Spawn signal handlers (SIGINT via ctrl_c, SIGTERM on Unix)
4. Wait for any process to complete or fail:
   - Success: log and continue waiting for others
   - Error: log, cancel all remaining processes, break
   - Panic: cancel all remaining processes, break
   - Signal received: cancel all processes, break
5. Await JoinSet shutdown (waits for cancelled processes to finish)
6. Run all Closers concurrently with timeout
7. Exit process (code 0 on success, code 1 on error)
```

Key behavior: if **any** process errors, the `CancellationToken` is cancelled, which signals all other processes to stop. This provides fail-fast semantics across the entire application.

## Worker Integration Contract

Every worker crate exposes either `into_runner_process()` or `into_runner_processes()` to convert itself into one or more `AppProcess` values. The contract requires that each process:

1. Accept a `CancellationToken` parameter
2. Respect cancellation by stopping work when the token fires
3. Return `Result<(), anyhow::Error>`

There are two common patterns for how workers handle the cancellation token.

### Pattern 1: Delegate to an inner component that owns cancellation

The worker passes the token directly to an inner component (e.g., a NATS consumer or server) that already knows how to handle cancellation internally.

**Used by:** `GatewayOrchestrator`, `CdcWorker`, `AnalyticsWorker`, `DocumentSnapshotter`

```rust
// GatewayOrchestrator
pub fn into_runner_process(self) -> ponix_runner::AppProcess {
    Box::new({
        let cdc_consumer = self.cdc_consumer;
        move |ctx| Box::pin(async move { cdc_consumer.run(ctx).await })
    })
}
```

### Pattern 2: Use cancellation for graceful shutdown of a server

The worker uses the token to trigger graceful shutdown on a long-running server (e.g., Axum or Tonic).

**Used by:** `CollaborationServer`

```rust
pub fn into_runner_process(self) -> AppProcess {
    Box::new(move |cancellation_token| {
        Box::pin(async move {
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    cancellation_token.cancelled().await;
                })
                .await?;
            Ok(())
        })
    })
}
```

### Pattern 3: Return the process as a generic closure (no explicit `AppProcess` type)

The worker returns `impl FnOnce(CancellationToken) -> Pin<Box<...>>` instead of the boxed `AppProcess` type. The `Runner` builder methods accept any `FnOnce` with the right signature, so this works without explicit boxing at the call site.

**Used by:** `PonixApi`

```rust
pub fn into_runner_process(
    self,
) -> impl FnOnce(CancellationToken) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
    move |ctx| {
        Box::pin(async move { run_ponix_grpc_server(self.config, self.services, ctx).await })
    }
}
```

## Multi-Process Workers

Some workers produce multiple concurrent processes via `into_runner_processes()` returning `Vec<AppProcess>`. This is used when a single logical worker needs several independent loops:

| Worker | Processes | Purpose |
|--------|-----------|---------|
| `AnalyticsWorker` | 3 | Processed envelope consumer, raw envelope consumer, ClickHouse inserter commit loop |
| `DocumentSnapshotter` | 2 | NATS document update consumer, compaction/flush loop |

In `ponix_all_in_one`, these are registered with auto-incrementing names:

```rust
let snapshotter_processes = document_snapshotter.into_runner_processes();
for (i, process) in snapshotter_processes.into_iter().enumerate() {
    runner = runner.with_named_process(format!("document_snapshotter_{}", i), process);
}
```

## Assembly in ponix_all_in_one

The monolith binary assembles the full runner in `main.rs`:

```rust
let mut runner = Runner::new();

// Single-process workers
runner = runner.with_named_process("ponix_api", ponix_api.into_runner_process());
runner = runner.with_named_process("gateway_orchestrator", gateway_orchestrator.into_runner_process());
runner = runner.with_named_process("cdc_worker", cdc_worker.into_runner_process());
runner = runner.with_named_process("collaboration_server", collaboration_server.into_runner_process());

// Multi-process workers
for (i, process) in document_snapshotter.into_runner_processes().into_iter().enumerate() {
    runner = runner.with_named_process(format!("document_snapshotter_{}", i), process);
}
for (i, process) in analytics_ingester.into_runner_processes().into_iter().enumerate() {
    runner = runner.with_named_process(format!("analytics_worker_{}", i), process);
}

// Cleanup
runner = runner
    .with_closer(|| async { /* cancel orchestrator, close NATS, flush telemetry */ })
    .with_closer_timeout(Duration::from_secs(10));

runner.run().await;
```

This yields approximately 9 concurrent processes in the current deployment.

## Closer Functions

Closers run **after** all processes have stopped, regardless of whether shutdown was triggered by a signal or a process error. All closers run concurrently within the timeout window. Current closers handle:

- Cancelling the gateway orchestrator shutdown token (stops gateway deployments)
- Closing the NATS client connection
- Flushing OpenTelemetry traces and logs via `shutdown_telemetry()`

If closers exceed the timeout (default 10s), a warning is logged and the application exits anyway.

## Adding a New Worker

To integrate a new worker crate with the runner:

1. Depend on `ponix-runner` in your crate's `Cargo.toml`
2. Implement `into_runner_process(self) -> AppProcess` (or `into_runner_processes()` for multiple)
3. Ensure your process respects the `CancellationToken` for graceful shutdown
4. In `ponix_all_in_one/src/main.rs`, instantiate your worker and register it:
   ```rust
   runner = runner.with_named_process("my_worker", my_worker.into_runner_process());
   ```
5. If your worker holds resources that need cleanup, add a `.with_closer(...)` call

## Source Files

| File | Purpose |
|------|---------|
| `crates/runner/src/lib.rs` | Runner, AppProcess, Closer, NamedProcess definitions |
| `crates/ponix_all_in_one/src/main.rs` | Full runner assembly |
| `crates/ponix_api/src/ponix_api.rs` | PonixApi runner integration |
| `crates/gateway_orchestrator/src/gateway_orchestrator.rs` | GatewayOrchestrator runner integration |
| `crates/cdc_worker/src/cdc_worker.rs` | CdcWorker runner integration |
| `crates/analytics_worker/src/analytics_worker.rs` | AnalyticsWorker multi-process runner integration |
| `crates/collaboration_server/src/collaboration_server.rs` | CollaborationServer runner integration |
| `crates/collaboration_server/src/document_snapshotter.rs` | DocumentSnapshotter multi-process runner integration |
