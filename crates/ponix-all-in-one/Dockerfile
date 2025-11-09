# syntax=docker/dockerfile:1

# Build stage
FROM rust:1.91-slim-trixie AS builder

# Update ca-certificates in builder stage
RUN apt-get update && \
    apt-get install -y ca-certificates

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
