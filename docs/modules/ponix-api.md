---
title: Ponix API
description: gRPC API layer and domain services for the Ponix platform.
crate: ponix_api
category: module
related-files:
  - crates/ponix_api/src/lib.rs
  - crates/ponix_api/src/ponix_api.rs
  - crates/ponix_api/src/domain.rs
  - crates/ponix_api/src/grpc.rs
  - crates/ponix_api/src/domain/data_stream_service.rs
  - crates/ponix_api/src/domain/data_stream_definition_service.rs
  - crates/ponix_api/src/domain/organization_service.rs
  - crates/ponix_api/src/domain/gateway_service.rs
  - crates/ponix_api/src/domain/document_service.rs
  - crates/ponix_api/src/domain/user_service.rs
  - crates/ponix_api/src/domain/workspace_service.rs
  - crates/ponix_api/src/domain/raw_envelope_ingestion_service.rs
  - crates/ponix_api/src/grpc/server.rs
  - crates/ponix_api/src/grpc/data_stream_handler.rs
last-updated: 2026-03-18
---

# Ponix API

The `ponix_api` crate is the gRPC API layer for the Ponix platform. It implements the service layer between external gRPC requests and the domain model, handling request validation, authorization, business logic orchestration, and protocol translation. It exists to keep API concerns cleanly separated from infrastructure (repositories, messaging) while enforcing consistent authorization and validation across all endpoints.

## Overview

This crate has two primary responsibilities:

1. **Domain services** (`domain/`) -- Business logic orchestration. Each service owns the rules for one aggregate (e.g., "creating a data stream requires the organization, definition, and gateway to all exist and belong to the same org"). Services depend only on trait-based repositories and authorization providers from `common`, making them fully unit-testable with mocks.

2. **gRPC handlers** (`grpc/`) -- Thin translation layer. Each handler implements a tonic-generated service trait, extracts the authenticated user from the JWT in request metadata, maps protobuf request types to domain service request types, calls the domain service, maps the result back to protobuf, and converts any `DomainError` to an appropriate gRPC `Status` code.

The crate exposes a `PonixApi` struct that packages everything into a single runner process for the `ponix_all_in_one` monolith.

## Key Concepts

- **`PonixApi`** -- Top-level struct that holds all services and server config; converts itself into an `AppProcess` via `into_runner_process()` for the Runner framework.

- **`PonixApiServices`** -- Aggregation struct carrying all `Arc<dyn ...>` domain services and auth providers. Its `into_routes()` method wires every gRPC handler into a single `tonic::service::Routes`.

- **Domain service request types** (e.g., `CreateDataStreamRequest`) -- Validated structs using the `garde` crate. Each embeds a `user_id` field so the service layer can perform authorization checks. This is the "two-type pattern": the gRPC handler constructs these from the proto request plus the JWT-extracted user context.

- **`AuthorizationProvider`** -- Trait from `common::auth` that every service calls via `require_permission(user_id, org_id, resource, action)` before performing any operation. This centralizes RBAC enforcement.

- **`domain_error_to_status`** -- A single function in `common::grpc` that maps every `DomainError` variant to the correct gRPC status code, ensuring consistent error semantics across the entire API surface.

- **ID generation** -- All entities get their IDs generated at the domain service layer using `xid::new()`, not at the gRPC or repository layer.

- **`RawEnvelopeIngestionService`** -- Unlike other services, this one does not require JWT auth. It validates data stream ownership and publishes binary payloads to NATS for downstream processing.

## How It Works

### Request Lifecycle

Every gRPC request follows the same pipeline:

1. **Authentication**: The handler calls `extract_user_context(&request, auth_token_provider)` to decode the JWT from the `authorization` metadata header. This yields a `UserContext` with `user_id` and `email`.

2. **Proto-to-domain mapping**: The handler constructs a domain service request struct, injecting `user_id` from the JWT context.

3. **Validation**: The domain service calls `validate_struct(&request)` which runs garde's declarative validators. Failures return `DomainError::ValidationError`.

4. **Authorization**: The service calls `authorization_provider.require_permission(user_id, org_id, Resource::X, Action::Y)`. Failures return `DomainError::PermissionDenied`.

5. **Business rules**: The service enforces domain invariants (e.g., cross-validates organization, definition, and gateway ownership for data streams).

6. **Repository call**: The service calls the appropriate repository trait method.

7. **Domain-to-proto mapping**: The handler converts the returned domain entity to a protobuf message.

8. **Error mapping**: Any `DomainError` is converted to a gRPC `Status` via `domain_error_to_status`.

### Domain Services

| Service | Entities | Notable behavior |
|---|---|---|
| `OrganizationService` | Organization | Creator automatically gets Admin role via `assign_role` |
| `WorkspaceService` | Workspace | Validates parent organization exists and is active |
| `DataStreamService` | DataStream | Cross-validates organization, definition, and gateway ownership |
| `DataStreamDefinitionService` | DataStreamDefinition | Compiles CEL expressions at creation time, validates JSON Schema syntax |
| `GatewayService` | Gateway | Full CRUD with organization ownership validation |
| `DocumentService` | Document | Creates with empty Yrs CRDT state, manages graph-edge associations via Apache AGE |
| `UserService` | User | Registration with Argon2 hashing, JWT+refresh token login, token rotation |
| `RawEnvelopeIngestionService` | RawEnvelope | No JWT auth, validates data stream ownership, publishes to NATS |

### Server Setup

The `run_ponix_grpc_server` function delegates to `common::grpc::run_grpc_server`, which provides:
- gRPC-Web support (HTTP/1.1 for browser clients)
- CORS configuration
- Logging and tracing middleware
- gRPC reflection service

## Configuration

| Variable | Default | Description |
|---|---|---|
| `PONIX_GRPC_HOST` | `0.0.0.0` | Bind address |
| `PONIX_GRPC_PORT` | `50051` | Listen port |
| `PONIX_GRPC_WEB_ENABLED` | `true` | Enable gRPC-Web for browser clients |
| `PONIX_GRPC_CORS_ALLOWED_ORIGINS` | `*` | CORS origins, comma-separated or `*` for all |

## Related Documentation

- [Common](common.md) — Repository traits, domain entities, authorization interfaces, proto converters, gRPC middleware
- [Collaboration Server](collaboration-server.md) — Real-time document editing (complements the metadata-only document CRUD here)
- [Analytics Worker](analytics-worker.md) — Consumes raw envelopes published by `RawEnvelopeIngestionService`
- [All-in-One Service Binary](ponix-all-in-one.md) — Wires `PonixApi` into the monolith
