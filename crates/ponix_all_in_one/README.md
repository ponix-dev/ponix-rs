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
