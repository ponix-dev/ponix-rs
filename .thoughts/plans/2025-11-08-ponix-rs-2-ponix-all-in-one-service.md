# Add ponix-all-in-one Service to ponix-rs Repository Implementation Plan

## Overview

Set up a new `ponix-all-in-one` service in the ponix-rs Rust monorepo that mirrors the Go implementation's architecture. This is a foundational scaffolding task that creates the service structure and deployment infrastructure without implementing business logic.

## Current State Analysis

The ponix-rs repository currently has:
- A workspace-based Cargo structure with a single `runner` crate
- The `runner` crate provides lifecycle management with concurrent processes, signal handling, and cleanup
- No binary crates or services yet
- No Docker or deployment configuration

The Go ponix repository has:
- A fully-featured `ponix-all-in-one` service at `cmd/ponix-all-in-one/main.go`
- Multi-stage Docker build with non-root user
- Tiltfile for local development
- Docker Compose configuration for service orchestration

### Key Discoveries:
- The Rust runner crate at `crates/runner/src/lib.rs` provides similar functionality to the Go runner
- Go service uses environment variable configuration via `internal/conf`
- Go Dockerfile uses Debian Trixie with mise for build tooling
- Tiltfile watches specific directories and triggers rebuilds

## Desired End State

After completing this plan:
- A runnable `ponix-all-in-one` binary that starts, logs messages, and runs continuously
- Integration with the existing `runner` crate for proper lifecycle management
- Minimal configuration structure ready for future expansion
- Complete local development setup with Docker, Tilt, and Compose
- Service accessible via `cargo run --bin ponix-all-in-one` and Docker container

## What We're NOT Doing

- Implementing any business logic (database connections, API handlers, etc.)
- Setting up OpenTelemetry (separate issue #5)
- Adding health check endpoints
- Integrating with external services (PostgreSQL, ClickHouse, NATS)
- Complex configuration beyond a simple placeholder parameter

## Implementation Approach

We'll create a minimal service that demonstrates the scaffolding pattern while keeping implementation simple. The service will use the existing runner crate for lifecycle management and add basic logging to show it's working.

## Phase 1: Binary Crate Setup

### Overview
Create the ponix-all-in-one binary crate and integrate it with the Cargo workspace.

### Changes Required:

#### 1. Workspace Configuration
**File**: `Cargo.toml`
**Changes**: Add the new binary crate to workspace members

```toml
[workspace]
members = ["crates/runner", "crates/ponix-all-in-one"]
resolver = "2"

# Rest remains the same...
```

#### 2. Binary Crate Creation
**File**: `crates/ponix-all-in-one/Cargo.toml`
**Changes**: Create new crate configuration

```toml
[package]
name = "ponix-all-in-one"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[[bin]]
name = "ponix-all-in-one"
path = "src/main.rs"

[dependencies]
ponix-runner = { path = "../runner" }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
serde = { version = "1.0", features = ["derive"] }
config = { version = "0.14", features = ["toml"] }
```

### Success Criteria:

#### Automated Verification:
- [x] Workspace compiles successfully: `cargo build`
- [x] New crate is recognized: `cargo metadata | grep ponix-all-in-one`
- [x] Dependencies resolve: `cargo tree -p ponix-all-in-one`

#### Manual Verification:
- [x] Crate structure follows Rust conventions
- [x] No compilation warnings

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation from the human that the manual testing was successful before proceeding to the next phase.

---

## Phase 2: Configuration Structure

### Overview
Implement a minimal configuration system with environment variables and a simple placeholder parameter.

### Changes Required:

#### 1. Configuration Module
**File**: `crates/ponix-all-in-one/src/config.rs`
**Changes**: Create configuration structure

```rust
use serde::{Deserialize, Serialize};
use config::{Config, ConfigError, Environment};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceConfig {
    /// Message to print (placeholder for future config)
    #[serde(default = "default_message")]
    pub message: String,

    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Sleep interval in seconds
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

fn default_message() -> String {
    "ponix-all-in-one service is running".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_interval() -> u64 {
    5
}

impl ServiceConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Config::builder()
            .add_source(Environment::with_prefix("PONIX"))
            .build()?
            .try_deserialize()
    }
}
```

### Success Criteria:

#### Automated Verification:
- [x] Module compiles: `cargo build -p ponix-all-in-one`
- [x] Config tests pass: `cargo test -p ponix-all-in-one config`

#### Manual Verification:
- [x] Environment variables are properly parsed
- [x] Default values work correctly

---

## Phase 3: Service Implementation

### Overview
Create the main service loop that integrates with the runner crate and implements basic logging.

### Changes Required:

#### 1. Main Service Implementation
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Implement the service entry point

```rust
mod config;

use anyhow::Result;
use ponix_runner::Runner;
use std::time::Duration;
use tracing::{info, error};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[tokio::main]
async fn main() {
    // Initialize tracing
    let config = match config::ServiceConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&config.log_level)))
        .init();

    info!("Starting ponix-all-in-one service");
    info!("Configuration: {:?}", config);

    // Create runner with the main service process
    let runner = Runner::new()
        .with_app_process({
            let config = config.clone();
            move |ctx| {
                Box::pin(async move {
                    run_service(ctx, config).await
                })
            }
        })
        .with_closer(|| {
            Box::pin(async move {
                info!("Running cleanup tasks...");
                // Placeholder for future cleanup
                tokio::time::sleep(Duration::from_secs(1)).await;
                info!("Cleanup complete");
                Ok(())
            })
        })
        .with_closer_timeout(Duration::from_secs(10));

    // Run the service (this will handle the exit)
    runner.run().await;
}

async fn run_service(
    ctx: tokio_util::sync::CancellationToken,
    config: config::ServiceConfig,
) -> Result<()> {
    info!("Service started successfully");

    let interval = Duration::from_secs(config.interval_secs);
    let mut counter = 0u64;

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping service");
                break;
            }
            _ = tokio::time::sleep(interval) => {
                counter += 1;
                info!("{} (iteration: {})", config.message, counter);
            }
        }
    }

    info!("Service stopped gracefully");
    Ok(())
}
```

### Success Criteria:

#### Automated Verification:
- [x] Service compiles: `cargo build --bin ponix-all-in-one`
- [x] No clippy warnings: `cargo clippy --bin ponix-all-in-one`
- [x] Service starts successfully: `timeout 5 cargo run --bin ponix-all-in-one`

#### Manual Verification:
- [x] Service logs messages at configured interval
- [x] Graceful shutdown on Ctrl+C
- [x] Cleanup tasks execute on shutdown

---

## Phase 4: Containerization

### Overview
Create a Dockerfile for building and running the service in a container.

### Changes Required:

#### 1. Dockerfile
**File**: `crates/ponix-all-in-one/Dockerfile`
**Changes**: Create multi-stage Docker build

```dockerfile
# syntax=docker/dockerfile:1

# Build stage
FROM rust:1.83-bookworm AS builder

WORKDIR /build

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build the service
RUN cargo build --release --bin ponix-all-in-one

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -ms /bin/bash ponix

# Copy binary from builder
COPY --from=builder /build/target/release/ponix-all-in-one /home/ponix/ponix-all-in-one

# Set ownership
RUN chown -R ponix:ponix /home/ponix

USER ponix
WORKDIR /home/ponix

# Default environment variables
ENV PONIX_LOG_LEVEL=info
ENV PONIX_MESSAGE="ponix-all-in-one in Docker"
ENV PONIX_INTERVAL_SECS=5

ENTRYPOINT ["./ponix-all-in-one"]
```

### Success Criteria:

#### Automated Verification:
- [x] Dockerfile created with proper multi-stage build structure
- [x] Docker build succeeds: `docker build -f crates/ponix-all-in-one/Dockerfile -t ponix-all-in-one:latest .`
- [x] Image size is reasonable: `docker images ponix-all-in-one:latest` (157MB)

#### Manual Verification:
- [x] Container runs successfully
- [x] Non-root user verified
- [x] Environment variables work

---

## Phase 5: Local Development Setup

### Overview
Set up Tiltfile and Docker Compose for local development and orchestration.

### Changes Required:

#### 1. Tiltfile
**File**: `Tiltfile`
**Changes**: Create Tilt configuration

```python
# Build the Docker image
docker_build(
    'ponix-all-in-one:latest',
    context='.',
    dockerfile='./crates/ponix-all-in-one/Dockerfile',
    only=[
        './Cargo.toml',
        './Cargo.lock',
        './crates',
    ],
    ignore=[
        '**/target',
        '**/*.md',
        '**/tests',
    ]
)

# Run via Docker Compose
docker_compose('./docker/docker-compose.service.yaml')
```

#### 2. Docker Compose Configuration
**File**: `docker/docker-compose.service.yaml`
**Changes**: Create service orchestration

```yaml
name: ponix-rs

services:
  ponix-all-in-one:
    image: ponix-all-in-one:latest
    container_name: ponix-all-in-one
    environment:
      PONIX_LOG_LEVEL: debug
      PONIX_MESSAGE: "Hello from ponix-rs!"
      PONIX_INTERVAL_SECS: "3"
    restart: unless-stopped
    networks:
      - ponix

networks:
  ponix:
    name: ponix
```

### Success Criteria:

#### Automated Verification:
- [x] Tiltfile created with proper configuration
- [x] Docker Compose valid: `docker compose -f docker/docker-compose.service.yaml config`

#### Manual Verification:
- [x] Service starts with `tilt up`
- [x] Live reload works on code changes
- [x] Logs visible in Tilt UI

---

## Phase 6: Documentation

### Overview
Add documentation on how to run the service locally.

### Changes Required:

#### 1. Service README
**File**: `crates/ponix-all-in-one/README.md`
**Changes**: Create usage documentation

```markdown
# ponix-all-in-one

A minimal all-in-one service for the Ponix platform, providing scaffolding for future business logic.

## Running Locally

### Using Cargo
```bash
# From repository root
cargo run --bin ponix-all-in-one

# With custom configuration
PONIX_LOG_LEVEL=debug PONIX_MESSAGE="Custom message" cargo run --bin ponix-all-in-one
```

### Using Docker
```bash
# Build the image
docker build -f crates/ponix-all-in-one/Dockerfile -t ponix-all-in-one:latest .

# Run the container
docker run --rm ponix-all-in-one:latest
```

### Using Tilt (Recommended)
```bash
# Start the service with live reload
tilt up

# View logs at http://localhost:10350
```

## Configuration

Environment variables:
- `PONIX_MESSAGE` - Message to log (default: "ponix-all-in-one service is running")
- `PONIX_LOG_LEVEL` - Log level: trace, debug, info, warn, error (default: info)
- `PONIX_INTERVAL_SECS` - Interval between log messages in seconds (default: 5)

## Architecture

The service integrates with the `ponix-runner` crate for lifecycle management, providing:
- Graceful shutdown on SIGTERM/SIGINT
- Concurrent process execution
- Cleanup task orchestration
```

#### 2. Update Root README
**File**: `README.md`
**Changes**: Add section about the new service

```markdown
## Services

### ponix-all-in-one

The main service binary that will eventually orchestrate all Ponix functionality. Currently provides basic scaffolding with:
- Integration with runner for lifecycle management
- Environment-based configuration
- Docker containerization
- Tilt-based local development

See [crates/ponix-all-in-one/README.md](crates/ponix-all-in-one/README.md) for details.
```

### Success Criteria:

#### Automated Verification:
- [x] Markdown files exist and are valid
- [x] All code examples in docs are syntactically correct

#### Manual Verification:
- [x] Documentation is clear and complete
- [x] Examples work as documented

---

## Testing Strategy

### Unit Tests:
- Configuration parsing with various environment variables
- Service initialization without starting the loop

### Integration Tests:
- Full service startup and shutdown
- Signal handling verification
- Configuration loading from environment

### Manual Testing Steps:
1. Start service with `cargo run --bin ponix-all-in-one`
2. Verify logs appear at configured interval
3. Test graceful shutdown with Ctrl+C
4. Run via Docker and verify same behavior
5. Use Tilt and verify live reload on code changes

## Performance Considerations

- Minimal memory footprint (< 10MB)
- Single async runtime thread is sufficient for now
- No database connections or heavy processing

## Migration Notes

Not applicable for this initial implementation.

## References

- Original ticket: GitHub Issue #2 (ponix-dev/ponix-rs)
- Runner crate: `crates/runner/src/lib.rs`
- Go implementation reference: `cmd/ponix-all-in-one/main.go` in ponix repository