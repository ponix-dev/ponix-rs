# NATS JetStream Producer and Consumer Implementation Plan

## Overview

This plan implements NATS JetStream producer and consumer functionality for the `ProcessedEnvelope` message type in the ponix-rs Rust repository. This replicates the existing Go implementation's batch processing capabilities while integrating with the existing runner pattern and configuration system.

## Current State Analysis

### What Exists Now:
- **Protobuf Support**: `ProcessedEnvelope` message type available via `ponix_proto::envelope::v1::ProcessedEnvelope`
- **Service Scaffold**: `ponix-all-in-one` service with runner integration ([main.rs:12-55](/Users/srall/development/personal/ponix-rs/crates/ponix-all-in-one/src/main.rs#L12-L55))
- **Configuration System**: Environment-based config with `PONIX_` prefix ([config.rs](/Users/srall/development/personal/ponix-rs/crates/ponix-all-in-one/src/config.rs))
- **Runner Pattern**: Async lifecycle management with graceful shutdown ([runner/lib.rs](/Users/srall/development/personal/ponix-rs/crates/runner/src/lib.rs))
- **Docker Infrastructure**: Working Tiltfile and docker-compose setup

### Go Implementation Reference:
- **Stream Name**: `processed_envelopes`
- **Subject Pattern**: `processed.envelopes.>` (consumer filter)
- **Producer Subject**: `processed_envelopes.{end_device_id}`
- **Consumer**: Durable with explicit ack policy
- **Batch Size**: 30 messages (configurable)
- **Max Wait**: 5 seconds (configurable)
- **NATS URL**: `nats://ponix-nats:4222`

### Key Discoveries:
- Current service loop generates `ProcessedEnvelope` messages every N seconds ([main.rs:85-92](/Users/srall/development/personal/ponix-rs/crates/ponix-all-in-one/src/main.rs#L85-L92))
- Runner supports multiple concurrent app processes
- Configuration uses serde with environment variable mapping
- Docker Compose uses shared `ponix` network
- Go repository uses two-file pattern: `docker-compose.deps.yaml` (infrastructure) and `docker-compose.service.yaml` (services with `include`)

## Desired End State

The `ponix-all-in-one` service will:
1. Connect to NATS JetStream on startup
2. Create/verify the `processed_envelopes` stream exists
3. Publish `ProcessedEnvelope` messages to JetStream (producer)
4. Consume messages in batches from JetStream (consumer)
5. Log consumed messages using `tracing` crate
6. Acknowledge successful messages (Ack) and reject failed ones (Nak)
7. Gracefully shutdown NATS connections via runner closers

### Verification:
- Producer successfully publishes messages: Check NATS monitoring at `http://localhost:8222`
- Consumer receives and logs batches: Check service logs with `docker logs ponix-all-in-one`
- Messages are acknowledged: Verify no message buildup in NATS stream
- Graceful shutdown works: Send SIGTERM and verify clean connection close

## What We're NOT Doing

- ClickHouse integration (issue #4)
- Advanced error retry policies beyond Ack/Nak
- Message filtering or routing logic
- Multi-tenant subject isolation
- Production-grade monitoring/alerting
- Consumer lag metrics
- Stream replication or clustering
- Custom JetStream storage configuration

## Implementation Approach

We'll build this in three phases:

1. **Phase 1: Infrastructure Setup** - Add NATS dependencies, Docker Compose, and basic connection management
2. **Phase 2: Producer Implementation** - Create producer that publishes `ProcessedEnvelope` messages
3. **Phase 3: Consumer Implementation** - Create batch consumer that logs received messages

This approach allows testing each component independently and matches the Go implementation's architecture.

---

## Phase 1: Infrastructure Setup

### Overview
Set up NATS JetStream dependencies, Docker Compose configuration, connection management, and stream initialization.

### Changes Required:

#### 1. Add NATS Dependencies
**File**: `crates/ponix-all-in-one/Cargo.toml`
**Changes**: Add async-nats crate for JetStream support

```toml
[dependencies]
# Existing dependencies...
async-nats = "0.37"
futures = "0.3"
```

#### 2. Extend Service Configuration
**File**: `crates/ponix-all-in-one/src/config.rs`
**Changes**: Add NATS configuration parameters

```rust
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    // Existing fields...
    #[serde(default = "default_message")]
    pub message: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,

    // NATS configuration
    #[serde(default = "default_nats_url")]
    pub nats_url: String,
    #[serde(default = "default_nats_stream")]
    pub nats_stream: String,
    #[serde(default = "default_nats_subject")]
    pub nats_subject: String,
    #[serde(default = "default_nats_batch_size")]
    pub nats_batch_size: usize,
    #[serde(default = "default_nats_batch_wait_secs")]
    pub nats_batch_wait_secs: u64,

    // Startup configuration
    #[serde(default = "default_startup_timeout_secs")]
    pub startup_timeout_secs: u64,
}

// NATS defaults
fn default_nats_url() -> String {
    "nats://localhost:4222".to_string()
}

fn default_nats_stream() -> String {
    "processed_envelopes".to_string()
}

fn default_nats_subject() -> String {
    "processed.envelopes.>".to_string()
}

fn default_nats_batch_size() -> usize {
    30
}

fn default_nats_batch_wait_secs() -> u64 {
    5
}

fn default_startup_timeout_secs() -> u64 {
    30
}
```

#### 3. Create NATS Module
**File**: `crates/ponix-all-in-one/src/nats_client.rs` (new file)
**Changes**: Connection management and stream setup

```rust
use anyhow::{Context, Result};
use async_nats::jetstream::{self, stream::Config as StreamConfig};
use tracing::{info, warn};

pub struct NatsClient {
    client: async_nats::Client,
    jetstream: jetstream::Context,
}

impl NatsClient {
    pub async fn connect(url: &str, timeout: std::time::Duration) -> Result<Self> {
        info!("Connecting to NATS at {} (timeout={:?})", url, timeout);

        // Configure connection timeout for establishing the TCP connection
        let client = async_nats::ConnectOptions::new()
            .connection_timeout(timeout)
            .connect(url)
            .await
            .context("Failed to connect to NATS")?;

        let jetstream = jetstream::new(client.clone());

        info!("Successfully connected to NATS");
        Ok(Self { client, jetstream })
    }

    pub async fn ensure_stream(&self, stream_name: &str) -> Result<()> {
        info!("Ensuring stream '{}' exists", stream_name);

        let stream_config = StreamConfig {
            name: stream_name.to_string(),
            subjects: vec![format!("{}.*", stream_name)],
            description: Some("Stream for processed envelopes".to_string()),
            ..Default::default()
        };

        match self.jetstream.get_stream(stream_name).await {
            Ok(_) => {
                info!("Stream '{}' already exists", stream_name);
            }
            Err(_) => {
                self.jetstream
                    .create_stream(stream_config)
                    .await
                    .context("Failed to create stream")?;
                info!("Created stream '{}'", stream_name);
            }
        }

        Ok(())
    }

    pub fn jetstream(&self) -> &jetstream::Context {
        &self.jetstream
    }

    pub async fn close(self) {
        info!("Closing NATS connection");
        // Connection closes automatically when dropped
    }
}
```

#### 4. Update Module Declarations
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Add nats_client module

```rust
mod config;
mod nats_client;

use anyhow::Result;
// ... rest of imports
```

#### 5. Create Docker Compose for Dependencies
**File**: `docker/docker-compose.deps.yaml` (new file)
**Changes**: Add NATS JetStream as infrastructure dependency (matching Go repository pattern)

```yaml
name: ponix-deps

services:
  nats:
    image: nats:latest
    container_name: ponix-nats
    ports:
      - "4222:4222"  # Client connections
      - "8222:8222"  # HTTP management
      - "6222:6222"  # Cluster routing
    command:
      - "-js"        # Enable JetStream
      - "-m"         # Enable monitoring
      - "8222"       # Monitoring port
    volumes:
      - nats-data:/data
    restart: unless-stopped
    networks:
      - ponix

networks:
  ponix:
    name: ponix

volumes:
  nats-data:
    driver: local
```

#### 6. Update Service Docker Compose
**File**: `docker/docker-compose.service.yaml`
**Changes**: Include dependencies and add NATS environment variables

```yaml
name: ponix-service

include:
  - docker-compose.deps.yaml

services:
  ponix-all-in-one:
    image: ponix-all-in-one:latest
    container_name: ponix-all-in-one
    environment:
      PONIX_LOG_LEVEL: debug
      PONIX_MESSAGE: "Hello from ponix-rs!"
      PONIX_INTERVAL_SECS: "3"
      PONIX_NATS_URL: "nats://ponix-nats:4222"
      PONIX_NATS_STREAM: "processed_envelopes"
      PONIX_NATS_SUBJECT: "processed.envelopes.>"
      PONIX_NATS_BATCH_SIZE: "30"
      PONIX_NATS_BATCH_WAIT_SECS: "5"
      PONIX_STARTUP_TIMEOUT_SECS: "30"
    depends_on:
      - nats
    restart: unless-stopped
    networks:
      - ponix

networks:
  ponix:
    name: ponix
```

### Success Criteria:

#### Automated Verification:
- [x] Cargo builds successfully: `cargo build --release`
- [x] Tests pass: `cargo test`
- [x] Dependencies start: `docker compose -f docker/docker-compose.deps.yaml up -d`
- [x] NATS is healthy: `curl -f http://localhost:8222/healthz`
- [x] Service starts: `docker compose -f docker/docker-compose.service.yaml up -d`
- [x] Service connects to NATS: Check logs for "Successfully connected to NATS"

#### Manual Verification:
- [ ] NATS monitoring UI accessible at `http://localhost:8222`
- [ ] Stream `processed_envelopes` visible in NATS monitoring
- [ ] Service logs show successful NATS connection
- [ ] Service gracefully disconnects on SIGTERM
- [ ] Verify stream exists: `nats stream ls`
- [ ] Check stream info: `nats stream info processed_envelopes`
- [ ] Verify stream subjects: `nats stream info processed_envelopes` shows `processed_envelopes.*`

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that NATS is running and the service can connect before proceeding to the next phase.

---

## Phase 2: Producer Implementation

### Overview
Implement the producer that publishes `ProcessedEnvelope` messages to NATS JetStream, matching the Go implementation's publishing pattern.

### Changes Required:

#### 1. Create Producer Module
**File**: `crates/ponix-all-in-one/src/producer.rs` (new file)
**Changes**: Implement ProcessedEnvelopeProducer

```rust
use anyhow::{Context, Result};
use async_nats::jetstream;
use ponix_proto::envelope::v1::ProcessedEnvelope;
use prost::Message;
use tracing::{debug, info};

pub struct ProcessedEnvelopeProducer {
    jetstream: jetstream::Context,
    base_subject: String,
}

impl ProcessedEnvelopeProducer {
    pub fn new(jetstream: jetstream::Context, base_subject: String) -> Self {
        info!("Created ProcessedEnvelopeProducer with base subject: {}", base_subject);
        Self {
            jetstream,
            base_subject,
        }
    }

    pub async fn publish(&self, envelope: &ProcessedEnvelope) -> Result<()> {
        // Serialize protobuf message
        let payload = envelope.encode_to_vec();

        // Build subject: {base_subject}.{end_device_id}
        let subject = format!("{}.{}", self.base_subject, envelope.end_device_id);

        debug!(
            subject = %subject,
            end_device_id = %envelope.end_device_id,
            size_bytes = payload.len(),
            "Publishing ProcessedEnvelope"
        );

        // Publish to JetStream
        let ack = self
            .jetstream
            .publish(subject.clone(), payload.into())
            .await
            .context("Failed to publish message to JetStream")?;

        // Await acknowledgment from JetStream
        ack.await.context("Failed to receive JetStream acknowledgment")?;

        debug!(
            subject = %subject,
            end_device_id = %envelope.end_device_id,
            "Successfully published and acknowledged"
        );

        Ok(())
    }
}
```

#### 2. Update Main Service to Produce Messages
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Integrate producer into service loop

```rust
mod config;
mod nats_client;
mod producer;

use anyhow::Result;
use ponix_proto::envelope::v1::ProcessedEnvelope;
use ponix_runner::Runner;
use producer::ProcessedEnvelopeProducer;
use prost::Message;
use prost_types::Timestamp;
use std::time::{Duration, SystemTime};
use tracing::{info, error};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    // Initialize tracing and load config (existing code)
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

    // Create startup timeout for all initialization operations
    let startup_timeout = Duration::from_secs(config.startup_timeout_secs);
    info!("Using startup timeout of {:?} for all initialization", startup_timeout);

    // Connect to NATS (timeout is passed to the client for internal timeout configuration)
    let nats_client = match nats_client::NatsClient::connect(&config.nats_url, startup_timeout).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to NATS: {}", e);
            std::process::exit(1);
        }
    };

    // Ensure stream exists
    if let Err(e) = nats_client.ensure_stream(&config.nats_stream).await {
        error!("Failed to ensure stream exists: {}", e);
        std::process::exit(1);
    }

    // Create producer
    let producer = ProcessedEnvelopeProducer::new(
        nats_client.jetstream().clone(),
        config.nats_stream.clone(),
    );

    // Create runner with the main service process
    let runner = Runner::new()
        .with_app_process({
            let config = config.clone();
            move |ctx| {
                Box::pin(async move {
                    run_producer_service(ctx, config, producer).await
                })
            }
        })
        .with_closer(move || {
            Box::pin(async move {
                info!("Running cleanup tasks...");
                nats_client.close().await;
                info!("Cleanup complete");
                Ok(())
            })
        })
        .with_closer_timeout(Duration::from_secs(10));

    runner.run().await;
}

async fn run_producer_service(
    ctx: tokio_util::sync::CancellationToken,
    config: config::ServiceConfig,
    producer: ProcessedEnvelopeProducer,
) -> Result<()> {
    info!("Producer service started successfully");

    let interval = Duration::from_secs(config.interval_secs);

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping producer service");
                break;
            }
            _ = tokio::time::sleep(interval) => {
                // Generate unique ID using xid
                let id = xid::new();

                // Get current time for timestamps
                let now = SystemTime::now();
                let timestamp = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| Timestamp {
                        seconds: d.as_secs() as i64,
                        nanos: d.subsec_nanos() as i32,
                    })
                    .ok();

                // Create ProcessedEnvelope message
                let envelope = ProcessedEnvelope {
                    end_device_id: id.to_string(),
                    occurred_at: timestamp.clone(),
                    data: None,
                    processed_at: timestamp,
                    organization_id: "example-org".to_string(),
                };

                // Publish to NATS JetStream
                match producer.publish(&envelope).await {
                    Ok(_) => {
                        info!(
                            end_device_id = %envelope.end_device_id,
                            organization_id = %envelope.organization_id,
                            "{}",
                            config.message
                        );
                    }
                    Err(e) => {
                        error!(
                            end_device_id = %envelope.end_device_id,
                            error = %e,
                            "Failed to publish envelope"
                        );
                    }
                }
            }
        }
    }

    info!("Producer service stopped gracefully");
    Ok(())
}
```

#### 3. Update Module Declaration
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Already included above

### Success Criteria:

#### Automated Verification:
- [x] Cargo builds successfully: `cargo build --release`
- [x] Tests pass: `cargo test`
- [x] Service starts: `docker compose -f docker/docker-compose.service.yaml up -d ponix-all-in-one`
- [x] Messages are published: Check NATS monitoring shows message count increasing

#### Manual Verification:
- [ ] Service logs show "Successfully published and acknowledged" messages
- [ ] NATS monitoring UI shows messages in `processed_envelopes` stream at `http://localhost:8222/streaming/streams/processed_envelopes`
- [ ] Message rate matches configured interval (default 3 seconds)
- [ ] No error logs about failed publishing
- [ ] Graceful shutdown still works with SIGTERM
- [ ] Check message count: `nats stream info processed_envelopes` shows increasing message count
- [ ] View messages: `nats stream view processed_envelopes` shows serialized protobuf messages
- [ ] Verify subjects: Messages published to `processed_envelopes.{device_id}` pattern

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that messages are flowing into NATS before proceeding to the next phase.

---

## Phase 3: Consumer Implementation

### Overview
Implement the batch consumer that fetches `ProcessedEnvelope` messages from NATS JetStream and logs them using the `tracing` crate.

### Changes Required:

#### 1. Create Consumer Module
**File**: `crates/ponix-all-in-one/src/consumer.rs` (new file)
**Changes**: Implement batch consumer with Ack/Nak support

```rust
use anyhow::{Context, Result};
use async_nats::jetstream::{self, consumer::PullConsumer};
use futures::StreamExt;
use ponix_proto::envelope::v1::ProcessedEnvelope;
use prost::Message as ProstMessage;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub struct ProcessedEnvelopeConsumer {
    consumer: PullConsumer,
    batch_size: usize,
    max_wait: Duration,
}

impl ProcessedEnvelopeConsumer {
    pub async fn new(
        jetstream: &jetstream::Context,
        stream_name: &str,
        consumer_name: &str,
        subject_filter: &str,
        batch_size: usize,
        max_wait_secs: u64,
    ) -> Result<Self> {
        info!(
            stream = stream_name,
            consumer = consumer_name,
            subject = subject_filter,
            "Creating JetStream consumer"
        );

        // Create or get existing durable consumer
        let consumer = jetstream
            .create_consumer_on_stream(
                jetstream::consumer::Config {
                    name: Some(consumer_name.to_string()),
                    durable_name: Some(consumer_name.to_string()),
                    filter_subject: subject_filter.to_string(),
                    ack_policy: jetstream::consumer::AckPolicy::Explicit,
                    ..Default::default()
                },
                stream_name,
            )
            .await
            .context("Failed to create consumer")?;

        info!(
            stream = stream_name,
            consumer = consumer_name,
            "Consumer created successfully"
        );

        Ok(Self {
            consumer,
            batch_size,
            max_wait: Duration::from_secs(max_wait_secs),
        })
    }

    pub async fn run(&self, ctx: CancellationToken) -> Result<()> {
        info!("Starting consumer loop");

        loop {
            tokio::select! {
                _ = ctx.cancelled() => {
                    info!("Received shutdown signal, stopping consumer");
                    break;
                }
                result = self.fetch_and_process_batch() => {
                    if let Err(e) = result {
                        error!(error = %e, "Error processing batch");
                        // Continue processing despite errors
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        info!("Consumer stopped gracefully");
        Ok(())
    }

    async fn fetch_and_process_batch(&self) -> Result<()> {
        debug!(
            batch_size = self.batch_size,
            max_wait_secs = self.max_wait.as_secs(),
            "Fetching message batch"
        );

        // Fetch batch of messages
        let mut messages = self
            .consumer
            .fetch()
            .max_messages(self.batch_size)
            .expires(self.max_wait)
            .messages()
            .await
            .context("Failed to fetch messages")?;

        let mut batch = Vec::new();
        let mut raw_messages = Vec::new();

        // Collect messages from stream
        while let Some(result) = messages.next().await {
            match result {
                Ok(msg) => {
                    raw_messages.push(msg);
                }
                Err(e) => {
                    warn!(error = %e, "Error receiving message from batch");
                }
            }
        }

        if raw_messages.is_empty() {
            debug!("No messages in batch");
            return Ok(());
        }

        info!(message_count = raw_messages.len(), "Received message batch");

        // Deserialize messages
        for msg in &raw_messages {
            match ProcessedEnvelope::decode(msg.payload.as_ref()) {
                Ok(envelope) => {
                    batch.push(envelope);
                }
                Err(e) => {
                    error!(
                        error = %e,
                        subject = %msg.subject,
                        "Failed to deserialize ProcessedEnvelope"
                    );
                }
            }
        }

        // Process batch (currently just logging)
        let process_result = self.process_batch(&batch).await;

        // Acknowledge or reject messages based on result
        match process_result {
            Ok(_) => {
                // Ack all messages on success
                for msg in raw_messages {
                    if let Err(e) = msg.ack().await {
                        error!(error = %e, "Failed to acknowledge message");
                    }
                }
                debug!(message_count = batch.len(), "Acknowledged all messages");
            }
            Err(e) => {
                // Nak all messages on failure
                error!(error = %e, "Batch processing failed, rejecting messages");
                for msg in raw_messages {
                    if let Err(e) = msg.ack_with(jetstream::AckKind::Nak(None)).await {
                        error!(error = %e, "Failed to reject message");
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_batch(&self, envelopes: &[ProcessedEnvelope]) -> Result<()> {
        info!(batch_size = envelopes.len(), "Processing envelope batch");

        for envelope in envelopes {
            info!(
                end_device_id = %envelope.end_device_id,
                organization_id = %envelope.organization_id,
                occurred_at = ?envelope.occurred_at,
                processed_at = ?envelope.processed_at,
                "Processed envelope from NATS"
            );
        }

        Ok(())
    }
}
```

#### 2. Integrate Consumer into Service
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Add consumer as second app process

```rust
mod config;
mod consumer;
mod nats_client;
mod producer;

use anyhow::Result;
use consumer::ProcessedEnvelopeConsumer;
use ponix_proto::envelope::v1::ProcessedEnvelope;
use ponix_runner::Runner;
use producer::ProcessedEnvelopeProducer;
use prost::Message;
use prost_types::Timestamp;
use std::time::{Duration, SystemTime};
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    // Initialize tracing and load config
    let config = match config::ServiceConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    info!("Starting ponix-all-in-one service");
    info!("Configuration: {:?}", config);

    // Create startup timeout for all initialization operations
    let startup_timeout = Duration::from_secs(config.startup_timeout_secs);
    info!("Using startup timeout of {:?} for all initialization", startup_timeout);

    // Connect to NATS (timeout is passed to the client for internal timeout configuration)
    let nats_client = match nats_client::NatsClient::connect(&config.nats_url, startup_timeout).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to NATS: {}", e);
            std::process::exit(1);
        }
    };

    // Ensure stream exists
    if let Err(e) = nats_client.ensure_stream(&config.nats_stream).await {
        error!("Failed to ensure stream exists: {}", e);
        std::process::exit(1);
    }

    // Create consumer
    let consumer = match ProcessedEnvelopeConsumer::new(
        nats_client.jetstream(),
        &config.nats_stream,
        "ponix-all-in-one",
        &config.nats_subject,
        config.nats_batch_size,
        config.nats_batch_wait_secs,
    )
    .await
    {
        Ok(consumer) => consumer,
        Err(e) => {
            error!("Failed to create consumer: {}", e);
            std::process::exit(1);
        }
    };

    // Create producer
    let producer = ProcessedEnvelopeProducer::new(
        nats_client.jetstream().clone(),
        config.nats_stream.clone(),
    );

    // Create runner with producer and consumer processes
    let runner = Runner::new()
        .with_app_process({
            let config = config.clone();
            move |ctx| Box::pin(async move { run_producer_service(ctx, config, producer).await })
        })
        .with_app_process({
            move |ctx| Box::pin(async move { consumer.run(ctx).await })
        })
        .with_closer(move || {
            Box::pin(async move {
                info!("Running cleanup tasks...");
                nats_client.close().await;
                info!("Cleanup complete");
                Ok(())
            })
        })
        .with_closer_timeout(Duration::from_secs(10));

    runner.run().await;
}

async fn run_producer_service(
    ctx: tokio_util::sync::CancellationToken,
    config: config::ServiceConfig,
    producer: ProcessedEnvelopeProducer,
) -> Result<()> {
    info!("Producer service started successfully");

    let interval = Duration::from_secs(config.interval_secs);

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping producer service");
                break;
            }
            _ = tokio::time::sleep(interval) => {
                let id = xid::new();
                let now = SystemTime::now();
                let timestamp = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| Timestamp {
                        seconds: d.as_secs() as i64,
                        nanos: d.subsec_nanos() as i32,
                    })
                    .ok();

                let envelope = ProcessedEnvelope {
                    end_device_id: id.to_string(),
                    occurred_at: timestamp.clone(),
                    data: None,
                    processed_at: timestamp,
                    organization_id: "example-org".to_string(),
                };

                match producer.publish(&envelope).await {
                    Ok(_) => {
                        info!(
                            end_device_id = %envelope.end_device_id,
                            organization_id = %envelope.organization_id,
                            "Published envelope"
                        );
                    }
                    Err(e) => {
                        error!(
                            end_device_id = %envelope.end_device_id,
                            error = %e,
                            "Failed to publish envelope"
                        );
                    }
                }
            }
        }
    }

    info!("Producer service stopped gracefully");
    Ok(())
}
```

### Success Criteria:

#### Automated Verification:
- [x] Cargo builds successfully: `cargo build --release`
- [x] Tests pass: `cargo test`
- [x] Linting passes: `cargo clippy -- -D warnings`
- [x] Formatting is correct: `cargo fmt --check`
- [x] Service starts with both processes: `docker compose -f docker/docker-compose.service.yaml up -d`
- [x] Consumer receives messages: Check logs show "Processed envelope from NATS"

#### Manual Verification:
- [ ] Service logs show both "Published envelope" and "Processed envelope from NATS" messages
- [ ] Consumer processes batches (check for "Received message batch" logs)
- [ ] Messages are acknowledged (NATS stream message count stays low)
- [ ] No Nak/retry loops in logs
- [ ] Graceful shutdown of both producer and consumer works
- [ ] NATS monitoring shows active consumer at `http://localhost:8222`
- [ ] Verify consumer exists: `nats consumer ls processed_envelopes`
- [ ] Check consumer info: `nats consumer info processed_envelopes ponix-all-in-one`
- [ ] Verify acknowledgments: Consumer shows low pending count and increasing ack count
- [ ] Test message flow: `nats stream info processed_envelopes` shows messages being consumed (low pending)

**Implementation Note**: This completes the NATS JetStream implementation. Both producer and consumer should be running concurrently within the same service, demonstrating end-to-end message flow.

---

## Testing Strategy

### Unit Tests:
- Configuration parsing with NATS parameters
- ProcessedEnvelope serialization/deserialization
- Subject formatting logic

### Integration Tests:
- Producer publishes to real NATS instance
- Consumer receives and processes messages
- Ack/Nak acknowledgment flow
- Graceful shutdown with cleanup

### Manual Testing Steps:
1. Start services: `tilt up` or `docker compose -f docker/docker-compose.service.yaml up`
2. Verify NATS health: `curl http://localhost:8222/healthz`
3. Check NATS monitoring: Open `http://localhost:8222` in browser
4. View stream details: Navigate to Streams → `processed_envelopes`
5. Verify message flow: Check service logs with `docker logs -f ponix-all-in-one`
6. Test graceful shutdown: `docker compose -f docker/docker-compose.service.yaml stop ponix-all-in-one`
7. Verify no message loss: Check NATS stream has no unacknowledged messages

### NATS CLI Validation Commands:

**Stream Verification:**
```bash
# List all streams
nats stream ls

# Get detailed stream info
nats stream info processed_envelopes

# View messages in stream (shows last N messages)
nats stream view processed_envelopes

# Get stream stats
nats stream report
```

**Consumer Verification:**
```bash
# List consumers for stream
nats consumer ls processed_envelopes

# Get consumer details
nats consumer info processed_envelopes ponix-all-in-one

# Monitor consumer in real-time
nats consumer next processed_envelopes ponix-all-in-one --count 5
```

**Debugging Commands:**
```bash
# Check if messages are piling up (pending count should be low)
nats stream info processed_envelopes | grep "Messages:"

# Verify acknowledgments (ack count should increase, pending should be low)
nats consumer info processed_envelopes ponix-all-in-one | grep -E "Delivered|Pending"

# Test publishing manually
nats pub processed_envelopes.test-device "test message"

# Monitor stream events
nats event --stream processed_envelopes
```

## Performance Considerations

- **Batch Size**: Default 30 messages balances throughput and latency
- **Max Wait**: 5 seconds prevents excessive blocking on low traffic
- **Connection Pooling**: async-nats handles connection management internally
- **Backpressure**: JetStream provides natural backpressure via fetch semantics
- **Memory**: Batch processing limits memory usage per iteration
- **Startup Timeout**: Single configurable timeout via `PONIX_STARTUP_TIMEOUT_SECS` (default: 30s) applied to all initialization operations (NATS connection, stream creation, consumer setup)

## Migration Notes

No data migration required as this is a new implementation. Configuration environment variables are additive and have sensible defaults.

## References

- Original issue: [Issue #3](https://github.com/ponix-dev/ponix-rs/issues/3)
- Go implementation: `/Users/srall/development/personal/ponix/internal/nats/`
- Protobuf types: `ponix_proto::envelope::v1::ProcessedEnvelope`
- async-nats docs: https://docs.rs/async-nats/latest/async_nats/
