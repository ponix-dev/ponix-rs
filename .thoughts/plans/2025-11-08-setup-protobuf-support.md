# Setup Protobuf Support in Rust using BSR and Prost - Implementation Plan

## Overview

Integrate protobuf support into ponix-rs by consuming the BSR-generated Rust SDK from `buf.build/ponix/ponix`. This uses pre-generated code from BSR's Cargo registry, eliminating the need for local code generation.

## Current State Analysis

**Repository Structure:**
- Workspace with 2 crates: `runner` (library) and `ponix-all-in-one` (binary)
- Modern Rust 2021 edition with Tokio async runtime
- No existing protobuf infrastructure (greenfield implementation)
- Clean workspace dependencies in root [Cargo.toml](../../Cargo.toml)
- Environment managed via [.mise.toml](../../.mise.toml) including BSR_TOKEN

**BSR Module:**
- Protobuf definitions at `buf.build/ponix/ponix`
- Rust SDK generation already configured
- BSR token authentication already set up in .mise.toml

### Key Discoveries:
- BSR provides Cargo registry at `sparse+https://buf.build/gen/cargo/`
- SDK naming: `ponix_ponix_community_neoeinstein-prost`
- No build.rs scripts needed - works like normal Cargo dependencies
- ProcessedEnvelope message will be used for verification
- xid crate available for globally unique, sortable IDs
- Docker secrets provide secure token handling
- mise manages BSR_TOKEN environment variable

## Desired End State

After completion:
1. Workspace configured with BSR Cargo registry
2. `ponix-all-in-one` imports and uses the generated SDK
3. Main service outputs ProcessedEnvelope values instead of simple print
4. Docker build uses secrets for secure BSR authentication
5. All code compiles without errors

**Verification:**
- Run `cargo build --workspace` - compiles successfully
- Run the service - outputs ProcessedEnvelope data with xid-based IDs
- Docker build works with BSR_TOKEN secret from mise

## What We're NOT Doing

- No gRPC/Connect-RPC implementation (future ticket)
- No local code generation (no build.rs)
- No Tonic integration (prost only)
- No service implementations
- No modifications to runner crate

## Implementation Approach

Use BSR's Generated SDK feature to consume pre-compiled Rust code as a normal Cargo dependency. This approach:
- Requires no protoc/buf CLI tooling in build
- Provides consistent generated code
- Enables simple `cargo build` workflow
- Uses Docker secrets for secure token management
- Leverages mise for environment configuration

## Phase 1: Configure Cargo BSR Registry

### Overview
Set up the ponix-rs repository to consume packages from the BSR Cargo registry.

### Changes Required:

#### 1. Create Cargo Registry Configuration
**File**: `.cargo/config.toml`

**Changes**: Add BSR registry configuration

```toml
[registries.buf]
index = "sparse+https://buf.build/gen/cargo/"
credential-provider = "cargo:token"
```

#### 2. Update .gitignore
**File**: `.gitignore`

**Changes**: Ensure Cargo credentials not committed (if not already present)

```gitignore
# Cargo authentication
.cargo/credentials.toml
```

#### 3. Add BSR Setup Documentation
**File**: `README.md`

**Changes**: Add BSR setup reference to prerequisites

Find the Prerequisites or Setup section and add:

```markdown
### BSR Authentication

This project uses protobuf types from the Buf Schema Registry. To set up access:

1. Ensure BSR_TOKEN is configured in `.mise.toml`
2. Follow the [BSR Cargo Registry setup guide](https://buf.build/docs/bsr/generated-sdks/cargo/)
3. Authenticate once per machine:
   ```bash
   cargo login --registry buf "Bearer $BSR_TOKEN"
   ```

The generated SDK workflow is documented in the BSR documentation.
```

### Success Criteria:

#### Automated Verification:
- [x] File `.cargo/config.toml` exists with correct content
- [x] `.gitignore` includes credentials exclusion
- [x] README updated with BSR reference
- [x] Run `cargo search --registry buf ponix_ponix` - ~~returns SDK package~~ (BSR doesn't support search, skipped)

#### Manual Verification:
- [ ] BSR registry configuration loads without errors
- [ ] Authentication persists in `~/.cargo/credentials.toml`
- [ ] BSR_TOKEN available via mise: `mise env | grep BSR_TOKEN`

---

## Phase 2: Add Protobuf Dependencies

### Overview
Add the BSR-generated SDK and required runtime dependencies to `ponix-all-in-one`.

### Changes Required:

#### 1. Add Dependencies to ponix-all-in-one
**File**: `crates/ponix-all-in-one/Cargo.toml`

**Changes**: Add protobuf and supporting dependencies

```toml
[dependencies]
# Existing dependencies...
ponix-runner.workspace = true
serde = { workspace = true, features = ["derive"] }
config = { version = "0.14", features = ["toml"] }

# Protobuf support
ponix-proto = { package = "ponix_ponix_community_neoeinstein-prost", registry = "buf" }
prost = "0.13"
xid = "1.2"
```

**Note**: Using `ponix-proto` as a readable alias for the generated SDK.

#### 2. Optionally Add to Workspace Dependencies
**File**: `Cargo.toml` (root)

**Changes**: Add shared dependencies to workspace

```toml
[workspace.dependencies]
# ... existing dependencies ...
prost = "0.13"
xid = "1.2"
```

Then update `crates/ponix-all-in-one/Cargo.toml`:
```toml
prost = { workspace = true }
xid = { workspace = true }
```

### Success Criteria:

#### Automated Verification:
- [x] Run `cargo fetch` - downloads SDK from BSR
- [x] Run `cargo tree -p ponix-proto` - shows dependency tree (used package name instead of alias)
- [x] Run `cargo check -p ponix-all-in-one` - compiles
- [x] `Cargo.lock` includes `ponix_ponix_community_neoeinstein-prost`, `prost`, and `xid`

#### Manual Verification:
- [ ] Generated code visible in `~/.cargo/registry/src/buf.build-*/`
- [ ] Can import `ponix_proto` types in code
- [ ] xid crate available for ID generation

---

## Phase 3: Update Service to Use ProcessedEnvelope

### Overview
Replace the simple print loop in the main service with code that creates and outputs ProcessedEnvelope data using xid for unique IDs.

### Changes Required:

#### 1. Update Service Main Loop
**File**: `crates/ponix-all-in-one/src/main.rs`

**Changes**: Import protobuf types and create ProcessedEnvelope

Add imports at top:
```rust
use ponix_proto::ponix::v1::ProcessedEnvelope;
use prost::Message;
use xid::Id;
```

Replace the current app_process closure (around line 20-35) with:

```rust
    runner = runner.with_app_process(|cancel_token| async move {
        loop {
            // Generate unique, sortable ID using xid
            let id = Id::new();

            // Create a ProcessedEnvelope message
            let envelope = ProcessedEnvelope {
                id: id.to_string(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                payload: b"example payload data".to_vec(),
                status: "processed".to_string(),
                ..Default::default()
            };

            // Log the envelope details
            tracing::info!(
                id = %envelope.id,
                timestamp = envelope.timestamp,
                payload_size = envelope.payload.len(),
                status = %envelope.status,
                "{}",
                config.message
            );

            // Optional: demonstrate serialization
            let encoded = envelope.encode_to_vec();
            tracing::debug!("Serialized envelope to {} bytes", encoded.len());

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(config.interval_secs)) => {},
                _ = cancel_token.cancelled() => {
                    tracing::info!("Service shutdown requested");
                    break;
                }
            }
        }
    });
```

### Success Criteria:

#### Automated Verification:
- [x] Run `cargo check -p ponix-all-in-one` - compiles without errors
- [x] Run `cargo clippy -p ponix-all-in-one` - no warnings
- [x] Run `cargo build --release -p ponix-all-in-one` - builds successfully

#### Manual Verification:
- [ ] Run service: `cargo run -p ponix-all-in-one`
- [ ] Output shows structured log with envelope fields:
  - `id` (xid format, 20-character string like `c7qrlt1b50g0000001mg`)
  - `timestamp` (Unix timestamp in seconds)
  - `payload_size` (20 bytes)
  - `status` ("processed")
- [ ] Debug logs show serialization byte count
- [ ] Service runs continuously until Ctrl+C
- [ ] Graceful shutdown works
- [ ] IDs are globally unique and sortable by time

**Implementation Note**: Adjust the `ProcessedEnvelope` field names based on the actual proto definition. The example assumes common fields - verify against generated SDK documentation. If the proto uses different field names or types, update accordingly.

---

## Phase 4: Update Docker Build with Secrets

### Overview
Ensure Docker build works with BSR authentication using Docker secrets for secure token management, pulling from mise environment.

### Changes Required:

#### 1. Update Dockerfile for BSR Auth with Secrets
**File**: `crates/ponix-all-in-one/Dockerfile`

**Changes**: Use Docker secrets for BSR authentication in build stage

```dockerfile
FROM rust:1.83-bookworm AS builder

WORKDIR /usr/src/ponix-rs

# Set up BSR authentication using Docker secret
RUN --mount=type=secret,id=bsr_token \
    if [ -f /run/secrets/bsr_token ]; then \
        cargo login --registry buf "Bearer $(cat /run/secrets/bsr_token)"; \
    fi

# Configure BSR registry
COPY .cargo/config.toml ./.cargo/config.toml

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build the binary
RUN cargo build --release -p ponix-all-in-one

# Runtime stage unchanged
FROM debian:bookworm-slim

RUN useradd -m -u 1000 ponix

COPY --from=builder /usr/src/ponix-rs/target/release/ponix-all-in-one /usr/local/bin/ponix-all-in-one

USER ponix

CMD ["ponix-all-in-one"]
```

#### 2. Update Docker Compose for Secrets
**File**: `docker/docker-compose.service.yaml`

**Changes**: Configure Docker secrets from environment

```yaml
services:
  ponix-all-in-one:
    build:
      context: ..
      dockerfile: crates/ponix-all-in-one/Dockerfile
      secrets:
        - bsr_token
    # ... rest of config ...

secrets:
  bsr_token:
    environment: "BSR_TOKEN"
```

#### 3. Update .gitignore
**File**: `.gitignore`

**Changes**: Ensure environment and credential files not committed

```gitignore
# Environment files
.env
.env.local

# Cargo authentication
.cargo/credentials.toml
```

#### 4. Update Tiltfile for Secrets
**File**: `Tiltfile`

**Changes**: Pass BSR_TOKEN as Docker secret from mise environment

```python
# Read BSR token from environment (managed by mise)
bsr_token = os.getenv('BSR_TOKEN', '')

# Write token to a temporary secret file for Docker
secret_file = '.bsr_token_secret'
local('echo -n "{}" > {}'.format(bsr_token, secret_file))

docker_build(
    'ponix-all-in-one',
    context='.',
    dockerfile='crates/ponix-all-in-one/Dockerfile',
    secret=[secret_file + '=bsr_token']
)

# Clean up secret file after build
local('rm -f {}'.format(secret_file))
```

#### 5. Document Docker Build Requirements
**File**: `README.md`

**Changes**: Add Docker secrets setup documentation

In the "Running with Docker" or "Development" section:

```markdown
### Building with Docker

The Docker build requires BSR authentication via Docker secrets. The BSR_TOKEN is managed via `.mise.toml`.

Build and run with Docker Compose:
```bash
# mise automatically loads BSR_TOKEN from .mise.toml
docker compose -f docker/docker-compose.service.yaml up --build
```

Or with Tilt:
```bash
# mise loads the environment
tilt up
```

**Note**: Docker secrets are used to securely pass the BSR token during build without exposing it in build args or image layers. The token is configured in `.mise.toml`.
```

### Success Criteria:

#### Automated Verification:
- [x] `.gitignore` includes environment files
- [x] Dockerfile uses `RUN --mount=type=secret` syntax with `CARGO_REGISTRIES_BUF_TOKEN`
- [x] ~~Docker Compose defines `secrets` section~~ (Skipped - using Tilt instead)
- [x] ~~Run `docker compose -f docker/docker-compose.service.yaml build`~~ (Using Tilt - builds successfully via `tilt up`)
- [ ] Run container: outputs ProcessedEnvelope logs

#### Manual Verification:
- [ ] Build completes without BSR authentication errors
- [ ] Secret not visible in Docker image layers: `docker history ponix-all-in-one` shows no token
- [ ] Service runs in container with same output as local (xid-based IDs, envelope fields)
- [ ] Tilt development workflow works with BSR_TOKEN from mise
- [ ] Container logs show properly formatted envelope data
- [ ] Can verify secret is not in image: `docker run ponix-all-in-one env` shows no BSR_TOKEN

**Implementation Note**: Docker secrets are only mounted during the RUN command execution and are not persisted in the image layers, making this approach more secure than build args. The secret file approach in Tilt is temporary and cleaned up after build. BSR_TOKEN is managed centrally in .mise.toml.

---

## Testing Strategy

### Unit Tests:
- Test ProcessedEnvelope creation and serialization
- Test default values
- Test round-trip encode/decode
- Test xid ID generation produces valid IDs

### Integration Tests:
- Service starts and runs successfully
- Logs show expected envelope data
- Docker build and run work
- IDs are unique across multiple envelope creations

### Manual Testing Steps:
1. **Local Development**:
   ```bash
   cargo run -p ponix-all-in-one
   ```
   Verify: Logs show envelope with xid-format id, timestamp, payload_size, status

2. **ID Uniqueness**:
   - Let service run for 10+ seconds
   - Verify each log entry has a different ID
   - Verify IDs are sortable (later IDs > earlier IDs lexicographically)

3. **Docker Build with Secrets**:
   ```bash
   # mise loads BSR_TOKEN from .mise.toml
   docker compose -f docker/docker-compose.service.yaml up --build
   ```
   Verify: Container builds and runs with same output

4. **Secret Security Verification**:
   ```bash
   # Check that token is not in image
   docker history ponix-all-in-one
   docker run ponix-all-in-one env | grep BSR
   ```
   Verify: No BSR_TOKEN visible in either output

5. **Tilt Development**:
   ```bash
   # mise loads environment
   tilt up
   ```
   Verify: Service rebuilds and runs on code changes

6. **Serialization Verification**:
   - Check debug logs for serialized byte count
   - Verify it's non-zero and reasonable (typically 40-100 bytes for this example)

## Performance Considerations

- **Build Time**: First build downloads SDK (~1-5s), cached thereafter
- **Runtime**: No performance impact from protobuf usage
  - xid generation is very fast (ns/op)
  - Protobuf serialization minimal for small messages
- **Binary Size**: Minimal increase (~100-500KB for proto code + xid)
- **Memory**: No heap allocations for xid generation, protobuf uses efficient encoding
- **Security**: Docker secrets are mounted as tmpfs and never written to image layers

## References

- **Issue**: #1 - Setup Protobuf Support in Rust using BSR and Prost
- **BSR Module**: https://buf.build/ponix/ponix
- **BSR Cargo Guide**: https://buf.build/docs/bsr/generated-sdks/cargo/
- **Prost Plugin**: https://buf.build/community/neoeinstein-prost
- **xid Crate**: https://crates.io/crates/xid
- **xid GitHub**: https://github.com/kazk/xid-rs
- **Docker Secrets**: https://docs.docker.com/build/building/secrets/
- **mise**: https://mise.jdx.dev/
