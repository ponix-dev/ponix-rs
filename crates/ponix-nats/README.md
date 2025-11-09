# ponix-nats

Reusable NATS JetStream client and consumer components for ponix services.

## Overview

This crate provides generic, reusable components for working with NATS JetStream:

- **NatsClient**: Connection management, stream creation, and JetStream context access
- **NatsConsumer**: Generic batch consumer with fine-grained Ack/Nak control
- **ProcessingResult**: Type for specifying which messages to acknowledge vs reject
- **BatchProcessor**: Type alias for processor functions
- **ProcessedEnvelopeProducer**: (Feature: `processed-envelope`) Producer for ProcessedEnvelope protobuf messages
- **create_processed_envelope_processor**: (Feature: `processed-envelope`) Default processor for ProcessedEnvelope messages

## Features

### Core Features (Always Available)
- Generic consumer that works with any message type
- Fine-grained control over message acknowledgments (individual Ack/Nak per message)
- Batch processing with configurable batch size and wait times
- Graceful shutdown support via tokio CancellationToken
- Comprehensive error handling and logging

### Optional Features
- **`processed-envelope`**: Adds ProcessedEnvelope-specific producer and processor
  - Enables `ProcessedEnvelopeProducer` for publishing protobuf messages
  - Enables `create_processed_envelope_processor()` for consuming and logging ProcessedEnvelope messages
  - Requires: `ponix-proto`, `prost`, `xid`

## Usage

### 1. Connect to NATS

```rust
use ponix_nats::NatsClient;
use std::time::Duration;

let client = NatsClient::connect("nats://localhost:4222", Duration::from_secs(30)).await?;
client.ensure_stream("my_stream").await?;
```

### 2. Create a Processor

```rust
use ponix_nats::{BatchProcessor, ProcessingResult};
use async_nats::jetstream::Message;

fn create_my_processor() -> BatchProcessor {
    Box::new(|messages: &[Message]| {
        let messages = messages.to_vec();
        Box::pin(async move {
            let mut ack = Vec::new();
            let mut nak = Vec::new();

            for (idx, msg) in messages.iter().enumerate() {
                match process_message(msg).await {
                    Ok(_) => ack.push(idx),
                    Err(e) => nak.push((idx, Some(e.to_string()))),
                }
            }

            Ok(ProcessingResult::new(ack, nak))
        }) as futures::future::BoxFuture<'static, anyhow::Result<ProcessingResult>>
    })
}
```

### 3. Create and Run Consumer

```rust
use ponix_nats::NatsConsumer;
use tokio_util::sync::CancellationToken;

let processor = create_my_processor();

let consumer = NatsConsumer::new(
    client.jetstream(),
    "my_stream",
    "my_consumer",
    "my.subject.>",
    30,  // batch size
    5,   // max wait seconds
    processor,
).await?;

let ctx = CancellationToken::new();
consumer.run(ctx).await?;
```

## ProcessingResult

The `ProcessingResult` type gives you fine-grained control over message acknowledgments:

- **Ack individual messages**: Successfully processed messages
- **Nak individual messages**: Failed messages that should be redelivered
- **Include error details**: Attach error messages to Nak'd messages for debugging

### Helper Methods

```rust
// Acknowledge all messages
ProcessingResult::ack_all(count)

// Reject all messages
ProcessingResult::nak_all(count, Some("error message".to_string()))

// Custom ack/nak lists
ProcessingResult::new(ack_indices, nak_indices_with_errors)
```

## Using the `processed-envelope` Feature

Add to your `Cargo.toml`:

```toml
[dependencies]
ponix-nats = { path = "../ponix-nats", features = ["processed-envelope"] }
```

Then use the built-in producer and processor:

```rust
use ponix_nats::{
    create_processed_envelope_processor, ProcessedEnvelopeProducer, NatsClient, NatsConsumer
};

// Create producer
let producer = ProcessedEnvelopeProducer::new(
    client.jetstream().clone(),
    "processed_envelopes".to_string(),
);

// Publish a message
producer.publish(&envelope).await?;

// Create consumer with default processor
let processor = create_processed_envelope_processor();
let consumer = NatsConsumer::new(
    client.jetstream(),
    "processed_envelopes",
    "my-consumer",
    "processed_envelopes.>",
    30,
    5,
    processor,
).await?;
```

## Examples

See `crates/ponix-all-in-one/src/main.rs` for a complete example using the `processed-envelope` feature.
