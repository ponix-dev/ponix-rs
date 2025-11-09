# Ponix RS

A Rust implementation of the Ponix monorepo, featuring robust infrastructure components built with Tokio.

## Project Structure

This is a Cargo workspace containing multiple crates:

```
ponix-rs/
├── Cargo.toml           # Workspace configuration
└── crates/
    ├── runner/          # Concurrent application runner
    └── ponix-all-in-one/ # Main service binary
```

## Services

### ponix-all-in-one

The main service binary that will eventually orchestrate all Ponix functionality. Currently provides basic scaffolding with:
- Integration with runner for lifecycle management
- Environment-based configuration
- Docker containerization
- Tilt-based local development

See [crates/ponix-all-in-one/README.md](crates/ponix-all-in-one/README.md) for details.

## Crates

### Runner (`crates/runner`)

A concurrent application runner that manages long-running processes with graceful shutdown capabilities.

**Features:**
- Concurrent execution of multiple processes using Tokio
- Graceful shutdown on SIGTERM/SIGINT signals
- Configurable cleanup timeout
- Automatic cleanup execution regardless of process outcome
- Builder pattern API for easy configuration

**Example:**

```rust
use ponix_runner::Runner;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let runner = Runner::new()
        .with_app_process(|ctx| async move {
            loop {
                tokio::select! {
                    _ = ctx.cancelled() => {
                        println!("Shutting down gracefully");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        println!("Working...");
                    }
                }
            }
            Ok(())
        })
        .with_closer(|| async move {
            println!("Cleaning up resources");
            Ok(())
        })
        .with_closer_timeout(Duration::from_secs(5));

    runner.run().await;
}
```

## Getting Started

### Prerequisites

- Rust 1.70 or later
- Cargo

### BSR Authentication

This project uses protobuf types from the Buf Schema Registry. To set up access:

1. Ensure BSR_TOKEN is configured in `.mise.toml`
2. Follow the [BSR Cargo Registry setup guide](https://buf.build/docs/bsr/generated-sdks/cargo/)
3. Authenticate once per machine:
   ```bash
   cargo login --registry buf "Bearer $BSR_TOKEN"
   ```

The generated SDK workflow is documented in the BSR documentation.

### Building

Build all crates in the workspace:

```bash
cargo build
```

Build in release mode:

```bash
cargo build --release
```

### Running Tests

Run all tests:

```bash
cargo test
```

Run tests for a specific crate:

```bash
cargo test -p ponix-runner
```

### Running Examples

The runner crate includes examples demonstrating its usage:

```bash
cargo run --example basic_runner
```

Press Ctrl+C to trigger graceful shutdown and see the cleanup process in action.

## Development

### Building with Docker

The Docker build requires BSR authentication via Docker secrets. The BSR_TOKEN is managed via `.mise.toml`.

Build and run with Tilt (recommended for development):
```bash
# mise automatically loads BSR_TOKEN from .mise.toml
tilt up
```

**Note**: Docker secrets are used to securely pass the BSR token during build without exposing it in build args or image layers. The token is configured in `.mise.toml`.

### Adding a New Crate

1. Create a new directory under `crates/`
2. Add the crate path to `Cargo.toml` workspace members
3. Create the crate with `cargo init --lib` or `cargo init --bin`

### Workspace Dependencies

Common dependencies are defined in the workspace `Cargo.toml` and can be referenced in individual crates using:

```toml
[dependencies]
tokio.workspace = true
```

## License

MIT
