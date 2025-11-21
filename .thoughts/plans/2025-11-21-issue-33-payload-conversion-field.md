# Add Required Payload Conversion Field to End Devices - Implementation Plan

## Overview

Add a required `payload_conversion` field to end devices to support payload processing logic. This field will store conversion instructions as a string that will be used by the existing payload conversion logic.

## Current State Analysis

The device management system currently has:
- Domain types (`Device`, `CreateDeviceInput`, `CreateDeviceInputWithId`) in `ponix-domain` crate
- PostgreSQL persistence with `devices` table containing 5 columns
- gRPC API using BSR-managed protobuf definitions
- Clean separation between domain, infrastructure, and API layers following DDD principles

### Key Discoveries:
- Domain service generates device IDs using `xid` crate - [ponix-domain/src/end_device_service.rs:32](crates/ponix-domain/src/end_device_service.rs#L32)
- PostgreSQL uses `device_name` column instead of `name` - [ponix-postgres/src/models.rs:18](crates/ponix-postgres/src/models.rs#L18)
- Protobuf types are vendored via BSR at build time (version 0.4.0-20251117015401)
- Type conversions handle field name mappings between layers

## Desired End State

After implementation:
- All device entities will have a required `payload_conversion` field containing conversion instructions
- The field will be persisted in PostgreSQL and exposed via the gRPC API
- Existing device creation workflows will require providing payload conversion instructions
- The field will be available for use by the payload processing logic

### Verification:
- Creating a device via gRPC requires `payload_conversion` field
- Retrieved devices include the `payload_conversion` field with correct values
- Database schema includes the new column with NOT NULL constraint

## What We're NOT Doing

- NOT implementing the actual payload conversion logic (already exists in ponix-payload crate)
- NOT making the field optional or providing defaults
- NOT migrating existing device data (no existing production data)
- NOT adding validation for the payload conversion syntax (will be handled by execution layer)

## Implementation Approach

Following the existing DDD architecture, we'll update each layer bottom-up: domain → infrastructure → API. This ensures type safety and compilation checks at each step.

## Phase 1: Domain Layer Updates

### Overview
Update domain types to include the required `payload_conversion` field in all device-related structures.

### Changes Required:

#### 1. Domain Types
**File**: `crates/ponix-domain/src/types.rs`
**Changes**: Add `payload_conversion` field to three structs

After line 9 in Device struct:
```rust
pub struct Device {
    pub device_id: String,
    pub organization_id: String,
    pub name: String,
    pub payload_conversion: String,  // NEW FIELD
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

After line 17 in CreateDeviceInput struct:
```rust
pub struct CreateDeviceInput {
    pub organization_id: String,
    pub name: String,
    pub payload_conversion: String,  // NEW FIELD
}
```

After line 26 in CreateDeviceInputWithId struct:
```rust
pub struct CreateDeviceInputWithId {
    pub device_id: String,
    pub organization_id: String,
    pub name: String,
    pub payload_conversion: String,  // NEW FIELD
}
```

#### 2. Domain Service
**File**: `crates/ponix-domain/src/end_device_service.rs`
**Changes**: Pass payload_conversion through to repository

Update line 43 to include the new field:
```rust
let input = CreateDeviceInputWithId {
    device_id,
    organization_id: input.organization_id,
    name: input.name,
    payload_conversion: input.payload_conversion,  // NEW FIELD
};
```

### Success Criteria:

#### Automated Verification:
- [x] Domain crate compiles: `cargo build -p ponix-domain`
- [x] Domain unit tests pass: `cargo test -p ponix-domain --lib`

#### Manual Verification:
- [x] None required for this phase

---

## Phase 2: Database Schema Migration

### Overview
Add the `payload_conversion` column to the PostgreSQL `devices` table.

### Changes Required:

#### 1. Create Migration File
**File**: `crates/ponix-postgres/migrations/20251121000000_add_payload_conversion_to_devices.sql`
**Changes**: Create new migration file

```sql
-- Add payload_conversion column to devices table
ALTER TABLE devices
ADD COLUMN payload_conversion TEXT NOT NULL DEFAULT '';

-- Remove the default after adding the column
-- This allows the migration to work with existing rows if any
ALTER TABLE devices
ALTER COLUMN payload_conversion DROP DEFAULT;
```

### Success Criteria:

#### Automated Verification:
- [x] Migration runs successfully: `docker-compose -f docker/docker-compose.deps.yaml up -d && cargo run -p goose -- -dir crates/ponix-postgres/migrations up`
- [x] Database schema includes new column: `docker exec -it ponix-postgres psql -U ponix -d ponix -c "\d devices"`

#### Manual Verification:
- [x] Verify column is NOT NULL and has correct type

---

## Phase 3: PostgreSQL Repository Updates

### Overview
Update PostgreSQL models and repository implementation to handle the new field.

### Changes Required:

#### 1. Database Models
**File**: `crates/ponix-postgres/src/models.rs`
**Changes**: Add field to both model structs

Update Device struct (after line 10):
```rust
pub struct Device {
    pub device_id: String,
    pub organization_id: String,
    pub device_name: String,
    pub payload_conversion: String,  // NEW FIELD
}
```

Update DeviceRow struct (after line 19):
```rust
pub struct DeviceRow {
    pub device_id: String,
    pub organization_id: String,
    pub device_name: String,
    pub payload_conversion: String,  // NEW FIELD
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### 2. Repository Implementation
**File**: `crates/ponix-postgres/src/device_repo.rs`
**Changes**: Update SQL queries to include new field

Update INSERT query (lines 35-36):
```rust
let query = r#"
    INSERT INTO devices (device_id, organization_id, device_name, payload_conversion, created_at, updated_at)
    VALUES ($1, $2, $3, $4, $5, $6)
    RETURNING device_id, organization_id, device_name, payload_conversion, created_at, updated_at
"#;
```

Update query execution (line 43):
```rust
    &input.device_id,
    &input.organization_id,
    &input.name,
    &input.payload_conversion,  // NEW PARAMETER
    &now,
    &now,
```

Update SELECT query in get_device (lines 77-78):
```rust
let query = r#"
    SELECT device_id, organization_id, device_name, payload_conversion, created_at, updated_at
    FROM devices
    WHERE device_id = $1
"#;
```

Update DeviceRow construction (lines 87-93):
```rust
let device_row = DeviceRow {
    device_id: row.get(0),
    organization_id: row.get(1),
    device_name: row.get(2),
    payload_conversion: row.get(3),  // NEW FIELD
    created_at: row.get(4),
    updated_at: row.get(5),
};
```

Update SELECT query in list_devices (lines 106-107):
```rust
let query = r#"
    SELECT device_id, organization_id, device_name, payload_conversion, created_at, updated_at
    FROM devices
    WHERE organization_id = $1
    ORDER BY created_at DESC
"#;
```

Update DeviceRow construction in list_devices (lines 119-125):
```rust
DeviceRow {
    device_id: row.get(0),
    organization_id: row.get(1),
    device_name: row.get(2),
    payload_conversion: row.get(3),  // NEW FIELD
    created_at: row.get(4),
    updated_at: row.get(5),
}
```

#### 3. Type Conversions
**File**: `crates/ponix-postgres/src/conversions.rs`
**Changes**: Update all conversion functions

Update CreateDeviceInputWithId to Device conversion (after line 10):
```rust
impl From<CreateDeviceInputWithId> for models::Device {
    fn from(input: CreateDeviceInputWithId) -> Self {
        models::Device {
            device_id: input.device_id,
            organization_id: input.organization_id,
            device_name: input.name,
            payload_conversion: input.payload_conversion,  // NEW FIELD
        }
    }
}
```

Update DeviceRow to domain Device conversion (after line 22):
```rust
impl From<models::DeviceRow> for Device {
    fn from(row: models::DeviceRow) -> Self {
        Device {
            device_id: row.device_id,
            organization_id: row.organization_id,
            name: row.device_name,
            payload_conversion: row.payload_conversion,  // NEW FIELD
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}
```

Update Device to domain Device conversion (after line 35):
```rust
impl From<models::Device> for Device {
    fn from(device: models::Device) -> Self {
        Device {
            device_id: device.device_id,
            organization_id: device.organization_id,
            name: device.device_name,
            payload_conversion: device.payload_conversion,  // NEW FIELD
            created_at: None,
            updated_at: None,
        }
    }
}
```

### Success Criteria:

#### Automated Verification:
- [x] PostgreSQL crate compiles: `cargo build -p ponix-postgres`
- [x] Integration tests pass: `cargo test -p ponix-postgres --features integration-tests -- --test-threads=1`

#### Manual Verification:
- [x] None required for this phase

---

## Phase 4: gRPC Layer Updates

### Overview
Update gRPC conversions to handle the new field. Note: Protobuf definitions are already updated in BSR.

### Changes Required:

#### 1. Proto to Domain Conversions
**File**: `crates/ponix-grpc/src/conversions.rs`
**Changes**: Update conversion functions

Update to_create_device_input function (after line 27):
```rust
pub fn to_create_device_input(req: CreateEndDeviceRequest) -> CreateDeviceInput {
    CreateDeviceInput {
        organization_id: req.organization_id,
        name: req.name,
        payload_conversion: req.payload_conversion,  // NEW FIELD
    }
}
```

Update to_proto_device function (after line 36):
```rust
pub fn to_proto_device(device: Device) -> EndDevice {
    EndDevice {
        device_id: device.device_id,
        organization_id: device.organization_id,
        name: device.name,
        payload_conversion: device.payload_conversion,  // NEW FIELD
        created_at: datetime_to_timestamp(device.created_at),
        updated_at: datetime_to_timestamp(device.updated_at),
    }
}
```

#### 2. Update BSR Package Version
**File**: `crates/ponix-grpc/Cargo.toml`
**Changes**: Update to latest BSR package version that includes payload_conversion field

Note: The exact version will need to be determined from BSR after protobuf changes are published.

### Success Criteria:

#### Automated Verification:
- [x] gRPC crate compiles: `cargo build -p ponix-grpc`
- [x] All workspace tests pass: `cargo test --workspace`
- [x] Service starts successfully: `cargo run -p ponix-all-in-one`

#### Manual Verification:
- [x] None required for this phase

---

## Phase 5: End-to-End Testing

### Overview
Verify the complete implementation works through the gRPC API.

### Manual Testing Steps:

1. Start the infrastructure:
```bash
docker-compose -f docker/docker-compose.deps.yaml up -d
```

2. Run the service:
```bash
cargo run -p ponix-all-in-one
```

3. Create a device with payload_conversion:
```bash
grpcurl -plaintext -d '{
  "organization_id": "org-001",
  "name": "Test Sensor",
  "payload_conversion": "{ \"temperature\": input.temp_c * 1.8 + 32 }"
}' localhost:50051 ponix.end_device.v1.EndDeviceService/CreateEndDevice
```

4. Retrieve the device and verify payload_conversion is returned:
```bash
grpcurl -plaintext -d '{"device_id": "[DEVICE_ID_FROM_CREATE]"}' \
  localhost:50051 ponix.end_device.v1.EndDeviceService/GetEndDevice
```

5. List devices and verify payload_conversion is included:
```bash
grpcurl -plaintext -d '{"organization_id": "org-001"}' \
  localhost:50051 ponix.end_device.v1.EndDeviceService/ListEndDevices
```

6. Verify database persistence:
```bash
docker exec -it ponix-postgres psql -U ponix -d ponix \
  -c "SELECT device_id, payload_conversion FROM devices WHERE organization_id = 'org-001';"
```

### Success Criteria:

#### Automated Verification:
- [x] Integration test suite passes: `mise run test:integration`
- [x] Service health check passes

#### Manual Verification:
- [ ] Device creation requires payload_conversion field (returns error if missing)
- [ ] Created device returns with payload_conversion field populated
- [ ] GetEndDevice response includes payload_conversion
- [ ] ListEndDevices response includes payload_conversion for all devices
- [ ] Database shows payload_conversion persisted correctly

---

## Testing Strategy

### Unit Tests:
- Domain service tests: Verify CreateDeviceInput includes payload_conversion
- Repository mock tests: Update expectations to include new field
- Conversion tests: Verify all type mappings handle the new field

### Integration Tests:
- PostgreSQL repository: Test CRUD operations with payload_conversion
- End-to-end: Create device via gRPC and verify full round-trip

### Manual Testing Steps:
1. Test device creation without payload_conversion (should fail)
2. Test device creation with empty payload_conversion (should fail if validation added)
3. Test device creation with valid payload_conversion
4. Verify payload_conversion persists correctly in database
5. Verify GetEndDevice returns payload_conversion
6. Verify ListEndDevices includes payload_conversion for all devices

## Performance Considerations

- The payload_conversion field is stored as TEXT in PostgreSQL, which has no size limit
- Consider adding a reasonable length validation (e.g., 10KB) if conversion instructions become large
- No indexing needed on this field as it won't be used for queries

## Migration Notes

- The migration adds a NOT NULL constraint with a temporary default
- If any existing devices exist, they would get empty string as payload_conversion
- The default is then removed to enforce the constraint for new records
- In production, coordinate migration timing with deployment

## References

- Original issue: [GitHub Issue #33](https://github.com/ponix-dev/ponix-rs/issues/33)
- BSR Package: `ponix_ponix_community_neoeinstein-prost`
- Related: Payload processing logic in `ponix-payload` crate