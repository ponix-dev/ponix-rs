# syntax=docker/dockerfile:1

# ============================================================================
# Chef stage - prepare recipe for dependency caching
# ============================================================================
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /build

# ============================================================================
# Planner stage - analyze dependencies
# ============================================================================
FROM chef AS planner

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY .cargo/config.toml ./.cargo/config.toml

# Create recipe (dependency manifest)
RUN cargo chef prepare --recipe-path recipe.json

# ============================================================================
# Builder stage - compile dependencies then source
# ============================================================================
FROM chef AS builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Install Goose binary for migrations
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

# Copy recipe from planner
COPY --from=planner /build/recipe.json recipe.json

# Build dependencies only (this layer is cached unless Cargo.toml/Cargo.lock change)
RUN --mount=type=secret,id=bsr_token \
    --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git \
    if [ -f /run/secrets/bsr_token ]; then \
        export CARGO_REGISTRIES_BUF_TOKEN="Bearer $(cat /run/secrets/bsr_token)" && \
        cargo chef cook --release --recipe-path recipe.json; \
    else \
        echo "Error: BSR token secret not found" && exit 1; \
    fi

# Now copy source code (this invalidates from here, but deps are cached)
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build the application (only recompiles our crates, not dependencies)
RUN --mount=type=secret,id=bsr_token \
    --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git \
    if [ -f /run/secrets/bsr_token ]; then \
        export CARGO_REGISTRIES_BUF_TOKEN="Bearer $(cat /run/secrets/bsr_token)" && \
        cargo build --release --bin ponix-all-in-one; \
    else \
        echo "Error: BSR token secret not found" && exit 1; \
    fi

# ============================================================================
# Runtime stage - minimal production image
# Must use same Debian version as builder (trixie) for glibc compatibility
# ============================================================================
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -ms /bin/bash ponix

# Copy binary from builder
COPY --from=builder /build/target/release/ponix-all-in-one /home/ponix/ponix-all-in-one

# Copy goose binary from builder for migrations
COPY --from=builder /usr/local/bin/goose /usr/local/bin/goose

# Copy migration files
COPY crates/init_process/migrations/clickhouse /home/ponix/migrations/clickhouse
COPY crates/init_process/migrations/postgres /home/ponix/migrations/postgres

# Set ownership
RUN chown -R ponix:ponix /home/ponix

USER ponix
WORKDIR /home/ponix

ENTRYPOINT ["./ponix-all-in-one"]
