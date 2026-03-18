---
description: >
  Guide for updating protobuf messages and pulling new proto versions from the Buf Schema Registry (BSR).
  Use this skill when the user wants to update protobufs, add a new proto message or field, change a gRPC service definition,
  update proto dependencies, bump proto versions, sync protos, or says things like "update the proto",
  "add a field to the proto", "new gRPC endpoint", "pull latest protos", "bump proto version",
  "sync protobuf", or "update BSR". Also trigger when the user mentions ponix-protobuf, proto changes,
  or .proto files in the context of this project.
user-invocable: true
---

# Update Protobuf

This skill walks through the full workflow for updating protobuf definitions used by ponix-rs. The proto definitions live in a separate repository (`ponix-dev/ponix-protobuf`) and are published to the Buf Schema Registry (BSR), where ponix-rs pulls them as Cargo dependencies.

## Important: Work Order

Proto changes MUST happen in the protobuf repo first, be pushed to BSR, and only then consumed by ponix-rs. Never try to modify proto-generated code directly in this repo.

## Prerequisites

- The `ponix-dev/ponix-protobuf` repo must be cloned as a sibling directory (i.e., `../ponix-protobuf` relative to ponix-rs)
- The `buf` CLI must be installed (`brew install bufbuild/buf/buf`)
- BSR authentication must be configured (`buf registry login`)

Verify the sibling repo exists before proceeding:

```bash
ls ../ponix-protobuf
```

If it doesn't exist, tell the user they need to clone it:
```bash
git clone git@github.com:ponix-dev/ponix-protobuf.git ../ponix-protobuf
```

## Step 1: Make Changes in ponix-protobuf

Navigate to the sibling protobuf repo and make the necessary changes to `.proto` files.

```bash
cd ../ponix-protobuf
```

Common changes include:
- Adding, removing, or renaming message fields
- Creating new message types
- Adding new RPC methods to service definitions
- Creating new proto packages

This is not production software — the proto schema should match what the domain actually needs. Don't use `deprecated` or `reserved` fields. If a field no longer makes sense, just remove it. If a field needs renaming, rename it directly. Field numbers can be reused freely since there are no deployed consumers relying on wire compatibility.

After making changes:

1. **Lint the protos** to catch issues early:
   ```bash
   buf lint
   ```

2. **Commit and push** the changes to the protobuf repo:
   ```bash
   git add -A && git commit -m "feat: describe your proto changes"
   git push
   ```

## Step 2: Push to BSR

Push the updated protos to the Buf Schema Registry. This must be done from the ponix-protobuf repo:

```bash
cd ../ponix-protobuf
buf push
```

This publishes the new proto definitions and triggers code generation for all configured plugins (including the Rust prost and tonic plugins that ponix-rs depends on).

Note the commit reference that `buf push` outputs — you'll need it to find the new version.

## Step 3: Find the Latest Version

Back in ponix-rs, check what versions are available on the BSR. The packages ponix-rs uses are:

- `ponix_ponix_community_neoeinstein-prost` (aliased as `ponix-proto-prost`)
- `ponix_ponix_community_neoeinstein-tonic` (aliased as `ponix-proto-tonic`)

Search for the latest available versions:

```bash
cargo search ponix_ponix_community_neoeinstein-prost --registry buf
cargo search ponix_ponix_community_neoeinstein-tonic --registry buf
```

The version format looks like: `0.5.0-20260301231928-8dfaced60b62.2` where:
- `0.5.0` — semantic version
- `20260301231928` — timestamp of the BSR build
- `8dfaced60b62` — abbreviated commit hash
- `.2` — build number

Look for versions with a timestamp newer than the current one in `Cargo.toml`.

## Step 4: Update Cargo.toml

Update the workspace dependency versions in the **root** `Cargo.toml`. Both packages are defined in `[workspace.dependencies]`:

```toml
# Protobuf (BSR registry)
ponix-proto-prost = { package = "ponix_ponix_community_neoeinstein-prost", version = "NEW_VERSION", registry = "buf" }
ponix-proto-tonic = { package = "ponix_ponix_community_neoeinstein-tonic", version = "NEW_VERSION", registry = "buf" }
```

The prost and tonic packages may have different version suffixes (different build numbers) even for the same proto commit — update each to its own latest version.

## Step 5: Update Cargo.lock and Verify

After updating versions:

```bash
cargo update -p ponix_ponix_community_neoeinstein-prost
cargo update -p ponix_ponix_community_neoeinstein-tonic
cargo build
```
