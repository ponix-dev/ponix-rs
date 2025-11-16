# syntax=docker/dockerfile:1

# Build stage
FROM rust:1.91-slim-trixie AS builder

# Update ca-certificates and install build dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Install Goose binary
# Using specific version for reproducibility
# Detect architecture and download appropriate binary
RUN ARCH=$(dpkg --print-architecture) && \
    if [ "$ARCH" = "amd64" ]; then \
        GOOSE_ARCH="x86_64"; \
    elif [ "$ARCH" = "arm64" ]; then \
        GOOSE_ARCH="arm64"; \
    else \
        echo "Unsupported architecture: $ARCH" && exit 1; \
    fi && \
    curl -fsSL https://github.com/pressly/goose/releases/download/v3.22.1/goose_linux_${GOOSE_ARCH} \
    -o /usr/local/bin/goose && \
    chmod +x /usr/local/bin/goose

WORKDIR /build

# Configure BSR registry
COPY .cargo/config.toml ./.cargo/config.toml

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build the service with BSR authentication via secret
RUN --mount=type=secret,id=bsr_token \
    if [ -f /run/secrets/bsr_token ]; then \
        export CARGO_REGISTRIES_BUF_TOKEN="Bearer $(cat /run/secrets/bsr_token)" && \
        cargo build --release --bin ponix-all-in-one; \
    else \
        echo "Error: BSR token secret not found" && exit 1; \
    fi

# Runtime stage - use same Debian version as builder to match GLIBC
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -ms /bin/bash ponix

# Copy binary from builder
COPY --from=builder /build/target/release/ponix-all-in-one /home/ponix/ponix-all-in-one

# CRITICAL: Copy goose binary from builder for migrations
COPY --from=builder /usr/local/bin/goose /usr/local/bin/goose

# Copy migration files
COPY crates/ponix-clickhouse/migrations /home/ponix/migrations

# Set ownership
RUN chown -R ponix:ponix /home/ponix

USER ponix
WORKDIR /home/ponix

ENTRYPOINT ["./ponix-all-in-one"]
