---
title: Common Crate
description: Shared domain types, repository traits, and infrastructure clients used by all Ponix workspace crates.
crate: common
category: module
related-files:
  - crates/common/src/lib.rs
  - crates/common/src/domain.rs
  - crates/common/src/domain/result.rs
  - crates/common/src/postgres.rs
  - crates/common/src/nats.rs
  - crates/common/src/grpc.rs
  - crates/common/src/telemetry.rs
  - crates/common/src/clickhouse.rs
  - crates/common/src/proto.rs
  - crates/common/src/auth.rs
  - crates/common/src/cel.rs
  - crates/common/src/yrs.rs
  - crates/common/Cargo.toml
last-updated: 2026-03-18
---

# Common Crate

The `common` crate is the shared foundation of the Ponix workspace. It owns all domain entity definitions, repository trait contracts, infrastructure client wrappers, and cross-cutting concerns like telemetry, authentication, and gRPC middleware. Every other workspace crate depends on `common`; it depends on none of them. This boundary exists so that domain logic never couples to a specific worker or API surface, and so infrastructure details can be swapped without touching business rules.

## Overview

`common` serves three roles simultaneously:

1. **Domain kernel** -- defines the canonical entity types (`DataStream`, `Document`, `Gateway`, `Organization`, `User`, `Workspace`, envelopes) and the repository/producer traits that the rest of the system programs against.
2. **Infrastructure adapters** -- provides concrete clients for PostgreSQL (deadpool connection pool), ClickHouse (with LZ4 compression and experimental JSON type support), and NATS JetStream (publish, consume, stream management). These are thin wrappers that expose trait-based interfaces for testability.
3. **Cross-cutting middleware** -- houses gRPC server scaffolding (logging, OpenTelemetry tracing, gRPC-Web/CORS, error mapping, auth context extraction), the full telemetry initialization pipeline, JWT/password auth services, CEL expression compilation, Yrs CRDT document helpers, and validation via Garde and JSON Schema.

Because the monolith (`ponix_all_in_one`) composes all workers into a single binary, `common` is the natural place for anything that two or more crates would otherwise duplicate.

## Key Concepts

- **`DomainError` / `DomainResult<T>`** -- A single error enum covering every domain failure case (not-found, already-exists, validation, auth, repository). This enum is the only error type that crosses layer boundaries; gRPC and NATS layers map it to their own status codes.

- **Repository traits** (`DataStreamRepository`, `DocumentRepository`, `GatewayRepository`, `OrganizationRepository`, `UserRepository`, `WorkspaceRepository`, etc.) -- Async traits that define storage contracts. Annotated with `#[cfg_attr(any(test, feature = "testing"), mockall::automock)]` so unit tests get mock implementations for free. Infrastructure modules in `postgres/` provide the real implementations.

- **Producer traits** (`ProcessedEnvelopeProducer`, `RawEnvelopeProducer`) -- Async publish interfaces for the envelope pipeline. Decouples the analytics worker's business logic from NATS.

- **Two-type input pattern** -- External-facing create inputs (e.g., `RegisterUserRepoInput` with plaintext password) are distinct from internal repository inputs (e.g., `RegisterUserRepoInputWithId` with hashed password and generated ID). The domain service layer is responsible for the transformation, keeping ID generation and password hashing out of both the API and persistence layers.

- **`TowerConsumer`** -- A NATS JetStream consumer that processes messages one at a time through a Tower `Service` stack, enabling middleware composition (tracing, logging) on the consumer side, not just the publisher side.

- **`run_grpc_server()`** -- A reusable server builder that takes a `GrpcServerConfig`, pre-built `Routes`, and file descriptor sets. It wires up logging, OpenTelemetry tracing, optional gRPC-Web with CORS, and reflection, then runs until a `CancellationToken` fires.

- **`AuthTokenProvider` / `PasswordService`** -- Trait-based auth abstractions. `AuthTokenProvider` handles JWT generation/validation; `PasswordService` handles Argon2 hashing. Both are mockable for testing.

## How It Works

### Module Organization

The crate follows the project's module path convention (Rust 2018+ style, no `mod.rs`). Each top-level module has a corresponding `.rs` file that declares private submodules and re-exports their public items:

- **`domain/`** -- Entity types, repository/producer traits, `DomainError`
- **`postgres/`** -- `PostgresClient` (deadpool, configurable pool size), one repository implementation file per entity
- **`nats/`** -- `NatsClient`, `NatsJetStreamPublisher`, `TowerConsumer`, tracing middleware, W3C trace context propagation
- **`grpc/`** -- `run_grpc_server()`, `GrpcLoggingLayer`, `GrpcTracingLayer`, domain error → gRPC Status mapping, user context extraction from Bearer tokens
- **`telemetry/`** -- `init_telemetry()`, `shutdown_telemetry()`, `TraceContextLogProcessor` (injects trace_id/span_id into logs)
- **`clickhouse/`** -- `ClickHouseClient` (LZ4 compression, experimental JSON type support)
- **`proto/`** -- Bidirectional domain ↔ protobuf converters for all entity types
- **`auth/`** -- JWT, Argon2 password hashing, Casbin-backed RBAC authorization, refresh tokens
- **`cel/`** -- CEL expression compilation and execution, `cayenne_lpp_decode` built-in function
- **`yrs/`** -- `create_empty_document()`, `ROOT_TEXT_NAME` constant

### Design Decisions

**Why a single `DomainError` enum?** Every service and repository returns `DomainResult<T>`. A unified error type means the gRPC error mapper and NATS consumers can handle all failures with a single match. The tradeoff is a larger enum, but it avoids error-wrapping boilerplate across layers.

**Why trait-based repositories?** The `mockall::automock` annotation on every repository trait means any domain service can be unit-tested with mock repositories -- no Docker, no database. The `testing` feature flag gates mock generation so it is excluded from release builds.

**Why does `common` own both domain and infrastructure?** In a future microservice split, the domain types and traits would move to a standalone crate and the PostgreSQL/NATS implementations would live in their respective service crates. Today, co-locating them avoids circular dependencies while the system runs as a monolith. The trait boundaries already enforce the separation that matters.

**Why Tower middleware for both gRPC and NATS?** Using Tower `Layer`/`Service` for gRPC logging and tracing and for NATS consumption gives a consistent composable middleware model across both protocols. OpenTelemetry trace context propagates through W3C headers in both gRPC requests and NATS message headers.

## Related Documentation

- [Runner](runner.md) — Process orchestration framework
- [Analytics Worker](analytics-worker.md) — Primary consumer of envelope domain types and producer traits
- [CDC Worker](cdc-worker.md) — Uses proto converters and NATS publisher infrastructure
- [Collaboration Server](collaboration-server.md) — Uses Yrs utilities and document repository traits
