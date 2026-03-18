---
title: Collaboration Server
description: Real-time collaborative document editing via Yrs CRDTs, WebSocket transport, and NATS-based cross-instance relay.
crate: collaboration_server
category: module
related-files:
  - crates/collaboration_server/src/lib.rs
  - crates/collaboration_server/src/collaboration_server.rs
  - crates/collaboration_server/src/document_snapshotter.rs
  - crates/collaboration_server/src/domain/room_manager.rs
  - crates/collaboration_server/src/domain/document_room.rs
  - crates/collaboration_server/src/domain/relay_trait.rs
  - crates/collaboration_server/src/domain/snapshotter_service.rs
  - crates/collaboration_server/src/domain/compaction_worker.rs
  - crates/collaboration_server/src/domain/document_awareness.rs
  - crates/collaboration_server/src/domain/connected_user.rs
  - crates/collaboration_server/src/domain/content_extractor.rs
  - crates/collaboration_server/src/domain/protocol.rs
  - crates/collaboration_server/src/domain/active_document.rs
  - crates/collaboration_server/src/domain/awareness.rs
  - crates/collaboration_server/src/nats/nats_relay.rs
  - crates/collaboration_server/src/nats/document_update_consumer.rs
  - crates/collaboration_server/src/nats/document_update_service.rs
  - crates/collaboration_server/src/websocket/handler.rs
  - crates/collaboration_server/src/websocket/connection.rs
  - crates/collaboration_server/src/websocket/auth.rs
last-updated: 2026-03-18
---

# Collaboration Server

The collaboration server enables real-time collaborative document editing across multiple users and multiple server instances. It combines Yrs (the Rust port of Yjs CRDTs) for conflict-free merging, WebSocket transport for low-latency client communication, and NATS for cross-instance synchronization and durable persistence. The crate produces two independent runner processes -- the WebSocket server and the document snapshotter -- that are registered with the system's `Runner` orchestrator.

## Overview

This crate is the backbone of Ponix's multi-user editing experience. Its responsibilities split into two concerns:

**Live editing (CollaborationServer):** Accepts WebSocket connections from browser clients (Plate editor with slate-yjs), authenticates users via JWT, manages per-document in-memory Yrs `Doc` instances, broadcasts updates to co-editors on the same instance, and relays updates to other instances via NATS JetStream.

**Durable persistence (DocumentSnapshotter):** Consumes the same Yrs updates from the `document_sync` JetStream stream, maintains its own in-memory document cache, and periodically compacts the accumulated CRDT state into PostgreSQL along with extracted plaintext and HTML content. This single-writer design avoids write contention -- only the snapshotter writes Yrs state to the database.

## Key Concepts

- **`DocumentRoom`** -- An in-memory Yrs `Doc` with a set of connected WebSocket clients. Applies updates, computes diffs for initial sync, and broadcasts changes to local clients. One room exists per actively-edited document per server instance.

- **`RoomManager`** -- Manages the lifecycle of rooms. Lazily creates rooms on first connection by loading the document's persisted Yrs state from PostgreSQL, starts NATS subscriptions for cross-instance relay, and tears down rooms when the last client disconnects. Uses per-document creation locking to prevent duplicate I/O when multiple connections arrive simultaneously.

- **`DocumentRelay` (trait)** -- Abstraction over cross-instance pub/sub. The `NatsDocumentRelay` implementation publishes document updates to JetStream (`document_sync.{document_id}`) and awareness state to core NATS (`awareness.{document_id}`). Each instance tags messages with a UUID `X-Instance-Id` header and skips its own messages on the receiving side.

- **`DocumentAwarenessManager`** -- Server-authoritative presence and cursor tracking per document. Clients can only update their cursor position; identity fields (name, email, color) are set by the server based on the authenticated user. Uses a Yjs-compatible binary wire format so browser clients decode it natively.

- **`SnapshotterService`** -- In-memory cache of `ActiveDocument` instances that tracks dirty state. On first update for a document, loads the base state from PostgreSQL, then applies incremental updates in memory. The `compact_dirty_documents` method encodes the full Yrs state, extracts content text/HTML, and writes to PostgreSQL using an advisory lock to prevent conflicts.

- **`CompactionWorker`** -- Periodic timer (default 30s) that calls `compact_dirty_documents` and `evict_idle_documents`. Runs a final compaction on shutdown to avoid data loss.

- **`ConnectedUser`** -- Server-authoritative user identity resolved from the JWT-authenticated user ID. Derives a deterministic HSL cursor color from a SHA-256 hash of the user ID, ensuring the same user always gets the same color across sessions.

## How It Works

### WebSocket Connection Lifecycle

1. **HTTP upgrade:** A client connects to `/ws/documents/{document_id}`. The handler checks the document exists by calling `RoomManager::get_or_create_room` before upgrading. If the document does not exist, the server returns 404 without opening a WebSocket.

2. **Authentication:** The first WebSocket message must be a text message containing a raw JWT string. The server validates it via `AuthTokenProvider` with a 5-second timeout. On failure, the connection is closed with code 4001 and a human-readable reason.

3. **User identity resolution:** After authentication, the server looks up the user in the database to get their name and email, then derives a deterministic color. This ensures awareness data is server-authoritative -- clients cannot spoof their identity.

4. **Initial sync:** The server sends two messages to the client: a SyncStep1 (the room's current state vector) and a SyncStep2 (the full Yrs document state). This follows the standard Yjs sync protocol, allowing the client to merge with any local state it may have.

5. **Awareness setup:** The server registers the user in the `DocumentAwarenessManager`, sends the full awareness state (all connected users' presence) to the new client, and broadcasts the new user's presence to existing clients. The presence update is also published to NATS for cross-instance awareness.

6. **Message loop:** The connection splits into two concurrent tasks:
   - **Inbound:** Reads WebSocket messages. Sync updates are applied to the room's Yrs doc, broadcast to other local clients, and published to NATS JetStream. Awareness messages have their cursor data extracted and merged with the server-authoritative identity before broadcasting.
   - **Outbound:** A spawned task drains an mpsc channel and forwards messages to the WebSocket. The room writes to this channel when broadcasting.

7. **Disconnect cleanup:** When the WebSocket closes, the server removes the client from the room and awareness manager, broadcasts a removal update, and publishes it to NATS. If no clients remain, the room is destroyed: NATS subscriptions are cancelled and the room is removed from the manager.

### Cross-Instance Relay via NATS

The `NatsDocumentRelay` uses two different NATS transport mechanisms, chosen deliberately for their different durability guarantees:

- **Document updates** use JetStream (`document_sync.{document_id}`). This provides durable, ordered delivery. When a new room is created, the relay subscribes with a `DeliverPolicy::ByStartTime` set to the document's `updated_at` timestamp, replaying any updates that occurred since the last PostgreSQL snapshot.

- **Awareness updates** use core NATS (`awareness.{document_id}`). Awareness is ephemeral by design -- cursor positions and presence are transient state that is meaningless after a disconnect. Using core NATS (fire-and-forget) avoids unnecessary storage overhead.

Both channels include an instance ID header. The relay skips messages from its own instance to prevent echo loops.

### Document Snapshotter Pipeline

The snapshotter is the single writer of Yrs CRDT state to PostgreSQL, a deliberate architectural choice to avoid write conflicts from multiple collaboration server instances.

1. **NATS consumption:** The `DocumentUpdateConsumer` pulls messages from the `document_sync` JetStream stream. For each message, it parses the document ID from the subject and delegates to `SnapshotterService::apply_update`.

2. **In-memory caching:** On first update for a document, the service loads the current state from PostgreSQL, initializes a Yrs `Doc`, and caches it as an `ActiveDocument`. Subsequent updates apply directly to the cached doc.

3. **Compaction:** Every 30 seconds, the `CompactionWorker` triggers `compact_dirty_documents`. This collects all dirty documents, extracts their content (plaintext and basic HTML), encodes the full Yrs state, and writes each to PostgreSQL via `update_yrs_state` with an advisory lock.

4. **Content extraction:** During compaction, the `ContentExtractor` reads the root text type from the Yrs doc to produce `content_text` and `content_html`. This enables full-text search and previews without needing to decode the CRDT state.

5. **Idle eviction:** Documents idle longer than the threshold (default 5 minutes) and not dirty are evicted from memory.

## Configuration

| Environment Variable | Default | Description |
|---|---|---|
| `PONIX_COLLAB_HOST` | `0.0.0.0` | WebSocket server bind address |
| `PONIX_COLLAB_PORT` | `50052` | WebSocket server port |
| `PONIX_COLLAB_CORS_ALLOWED_ORIGINS` | `*` | CORS allowed origins (comma-separated or `*` for all) |
| `PONIX_NATS_DOCUMENT_UPDATES_STREAM` | `document_sync` | JetStream stream name for Yrs updates |
| `PONIX_NATS_DOCUMENT_UPDATES_CONSUMER_NAME` | `document-snapshotter` | Durable consumer name for the snapshotter |
| `PONIX_NATS_DOCUMENT_UPDATES_MAX_AGE_SECS` | `604800` | Max age for messages in the document_sync stream (7 days) |
| `PONIX_SNAPSHOTTER_COMPACTION_INTERVAL_SECS` | `30` | How often the compaction worker flushes dirty docs to PostgreSQL |
| `PONIX_SNAPSHOTTER_IDLE_EVICTION_SECS` | `300` | How long an idle, clean document stays in memory before eviction |

## Related Documentation

- [Runner](runner.md) — Process lifecycle and graceful shutdown
- [CDC Worker](cdc-worker.md) — Publishes document metadata CDC events (separate from the `document_sync` stream used here)
- [Common](common.md) — Domain types and repository traits
