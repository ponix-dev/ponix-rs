# ClickHouse Integration Test Findings

## Summary
While implementing ClickHouse batch writes for ProcessedEnvelope messages, we encountered several issues when attempting to use ClickHouse's native JSON type with the Rust client library.

## Environment
- **ClickHouse Versions Tested**: 23.8, 24.3, 24.10, 25.10 (latest)
- **clickhouse-rs Versions**: 0.13, 0.14
- **Test Framework**: testcontainers + goose migrations
- **Target Feature**: Store ProcessedEnvelope protobuf messages with JSON data field

## Issues Encountered

### 1. Binary Serialization Format Issue (clickhouse-rs 0.13)
**Status**: ❌ Unresolved with String type

**Symptoms**:
- Consistent pattern: N-1 rows write successfully, last row fails
- Error: `Cannot read all data. Bytes read: X. Bytes expected: Y.` (typically 9-10 bytes short)
- Occurred with both `insert()` and `inserter()` APIs
- Affected all ClickHouse versions tested (23.8, 24.3, 25.10)

**Schema Used**:
```sql
CREATE TABLE processed_envelopes (
  organization_id String NOT NULL,
  end_device_id String NOT NULL,
  occurred_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data String NOT NULL  -- JSON stored as String
)
```

**Conclusion**: Fundamental incompatibility between clickhouse-rs 0.13's binary serialization and ClickHouse servers when writing batches.

---

### 2. JSON Type Support Attempt (clickhouse-rs 0.13)
**Status**: ❌ JSON type not supported in clickhouse-rs 0.13

**Finding**: The ClickHouse Rust client 0.13 does not support the JSON data type, even with ClickHouse 24.10+ where JSON is stable.

**Error**:
```
Code: 117. DB::Exception: Unknown type code: 0x65
```

**Attempted Fixes**:
- Adding `allow_experimental_json_type=1` setting
- Using `input_format_binary_read_json_as_string=1`
- Using `output_format_binary_write_json_as_string=1`

**Result**: All attempts failed with binary format errors.

---

### 3. DateTime Type Mismatch (clickhouse-rs 0.14 + JSON type)
**Status**: ❌ Current Blocker

**Symptoms**:
After upgrading to clickhouse-rs 0.14 and using JSON type in schema:

```
While processing column ProcessedEnvelopeRow.occurred_at:
attempting to (de)serialize ClickHouse type DateTime as &str
which is not compatible
```

**Schema Used**:
```sql
CREATE TABLE processed_envelopes (
  organization_id String NOT NULL,
  end_device_id String NOT NULL,
  occurred_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data JSON NOT NULL  -- Using JSON type
)
```

**Rust Struct**:
```rust
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ProcessedEnvelopeRow {
    pub organization_id: String,
    pub end_device_id: String,
    pub occurred_at: DateTime<Utc>,  // Using chrono DateTime
    pub processed_at: DateTime<Utc>,
    pub data: String,  // JSON mapped to String in Rust
}
```

**Analysis**:
- clickhouse-rs 0.14 has stricter type validation
- When JSON type is present in the table, ClickHouse's schema introspection reports DateTime columns as string types
- This may be due to:
  1. Bug in ClickHouse 24.10's schema reporting with mixed JSON/DateTime columns
  2. Incompatibility between JSON type and binary format in clickhouse-rs 0.14
  3. Missing configuration for proper type handling with JSON columns

**Attempted Fixes**:
- ✅ Added `allow_experimental_json_type=1` to client options
- ✅ Added `allow_experimental_json_type=1` to goose migration DSN
- ✅ Upgraded to clickhouse-rs 0.14
- ✅ Fixed API changes (insert is now async)
- ❌ Removed `input_format_binary_read_json_as_string` settings (didn't help)

---

## What Works ✅

1. **Connection & Ping**: ClickHouse connection via HTTP works perfectly
2. **Migrations**: Goose migrations execute successfully with JSON type when `allow_experimental_json_type=1` is in DSN
3. **Empty Batches**: Writing empty batches succeeds (no actual data sent)
4. **Testcontainers Setup**: Infrastructure for integration testing is solid
5. **Test Helpers**: Migration runner and test data generators work correctly

---

## Potential Root Causes

### Theory 1: JSON Type Binary Format Incompatibility
The JSON type in ClickHouse 24.10+ may use a binary format that's incompatible with clickhouse-rs 0.13/0.14's binary row format implementation.

### Theory 2: Missing Serde Attributes for DateTime Fields
The DateTime example shows that DateTime fields require explicit serde attributes. Our implementation may be missing these.

### Theory 3: Incorrect Configuration Combination
We may have been combining settings incorrectly or applying them at the wrong level (client vs query vs migration).

**Reference Examples**: The clickhouse-rs repository contains working examples that demonstrate different features:

1. **JSON Type Usage** ([data_types_new_json.rs](https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_new_json.rs)):
   - **Requirements**: ClickHouse 24.10+
   - **Key Settings**:
     - `allow_experimental_json_type=1`
     - `input_format_binary_read_json_as_string=1`
     - `output_format_binary_write_json_as_string=1`
   - **Schema**: Simple table with `id UInt64` and `data JSON`

2. **DateTime Types with Row Derive** ([data_types_derive_simple.rs](https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_derive_simple.rs)):
   - Shows proper usage of DateTime, DateTime64 with various precisions
   - **Critical Pattern**: DateTime fields require explicit serde attributes:
     ```rust
     #[serde(with = "clickhouse::serde::chrono::datetime")]
     pub chrono_datetime: DateTime<Utc>,
     ```
   - Demonstrates both `time` crate and `chrono` crate DateTime mappings
   - Supports multiple precision levels (seconds, millis, micros, nanos)

**Gap in Examples**: Neither example demonstrates using JSON type and DateTime fields together in the same table. Our implementation attempts this combination, which may require combining patterns from both examples.

---

## Recommendations for Next Steps

### Option 1: Add Missing Serde Attributes (RECOMMENDED)
Add explicit serde attributes to DateTime fields as shown in the official examples:
```rust
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ProcessedEnvelopeRow {
    pub organization_id: String,
    pub end_device_id: String,
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub occurred_at: DateTime<Utc>,
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub processed_at: DateTime<Utc>,
    pub data: String,
}
```
- **Likelihood**: High - this is a documented requirement we missed
- **Effort**: Low - simple code change
- **Risk**: Low - follows official example pattern

### Option 2: Correct JSON Type Settings
Re-enable the `input_format_binary_read_json_as_string` and `output_format_binary_write_json_as_string` settings:
- May need to be applied at query level rather than client level
- Combine with serde attributes from Option 1
- Follow exact pattern from GitHub example

---

## Code Examples That Didn't Work

For reference, here's what was implemented that failed:

### Client Configuration (src/client.rs)
```rust
use anyhow::Result;
use clickhouse::Client;

#[derive(Clone)]
pub struct ClickHouseClient {
    client: Client,
}

impl ClickHouseClient {
    pub fn new(url: &str, database: &str, username: &str, password: &str) -> Self {
        let client = Client::default()
            .with_url(url)
            .with_database(database)
            .with_user(username)
            .with_password(password)
            .with_compression(clickhouse::Compression::Lz4)
            // Enable JSON type support for ClickHouse 24.10+
            .with_option("allow_experimental_json_type", "1");
            // Note: These settings were removed after testing showed they caused DateTime type mismatches:
            // .with_option("input_format_binary_read_json_as_string", "1")
            // .with_option("output_format_binary_write_json_as_string", "1")

        Self { client }
    }

    pub async fn ping(&self) -> Result<()> {
        self.client.query("SELECT 1").fetch_one::<u8>().await?;
        Ok(())
    }

    pub fn get_client(&self) -> &Client {
        &self.client
    }
}
```

### ProcessedEnvelopeRow Struct (src/envelope_store.rs)
**Problem**: Missing serde attributes on DateTime fields
```rust
use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ProcessedEnvelopeRow {
    pub organization_id: String,
    pub end_device_id: String,
    // ❌ MISSING: #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub occurred_at: DateTime<Utc>,
    // ❌ MISSING: #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub processed_at: DateTime<Utc>,
    pub data: String,  // JSON stored as String
}
```

### Batch Insert Implementation (src/envelope_store.rs)
```rust
pub async fn store_processed_envelopes(
    &self,
    envelopes: Vec<ProcessedEnvelope>,
) -> Result<()> {
    if envelopes.is_empty() {
        return Ok(());
    }

    let mut rows = Vec::with_capacity(envelopes.len());

    for envelope in envelopes {
        // Convert protobuf Struct to JSON string
        let data_json = match envelope.data {
            Some(ref struct_val) => {
                let fields_map: serde_json::Map<String, serde_json::Value> = struct_val
                    .fields
                    .iter()
                    .filter_map(|(k, v)| {
                        prost_value_to_json(v).map(|json_val| (k.clone(), json_val))
                    })
                    .collect();
                serde_json::to_string(&fields_map)?
            }
            None => "{}".to_string(),
        };

        let occurred_at = timestamp_to_datetime(
            envelope.occurred_at.as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing occurred_at timestamp"))?
        )?;

        let processed_at = timestamp_to_datetime(
            envelope.processed_at.as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing processed_at timestamp"))?
        )?;

        rows.push(ProcessedEnvelopeRow {
            organization_id: envelope.organization_id,
            end_device_id: envelope.end_device_id,
            occurred_at,
            processed_at,
            data: data_json,
        });
    }

    // Perform batch insert (clickhouse-rs 0.14 API)
    let mut insert = self
        .client
        .get_client()
        .insert::<ProcessedEnvelopeRow>(&self.table)
        .await?;

    for row in &rows {
        insert.write(row).await?;
    }

    insert.end().await?;
    Ok(())
}
```

### Migration Configuration (src/migration.rs)
```rust
pub async fn run_migrations(&self) -> Result<()> {
    // Build connection string for goose with JSON type support
    let dsn = format!(
        "clickhouse://{}:{}@{}/{}?allow_experimental_json_type=1",
        self.user, self.password, self.clickhouse_url, self.database
    );

    info!("Running database migrations...");

    let output = Command::new(&self.goose_binary_path)
        .args([
            "-dir", &self.migrations_dir,
            "clickhouse",
            &dsn,
            "up",
        ])
        .output()
        .context("Failed to run goose migrations")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Migration failed: {}", stderr);
    }

    Ok(())
}
```

### Table Schema (migrations/20251104110101_add_organization_id.sql)
```sql
-- +goose Up
-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS processed_envelopes_new (
  organization_id String NOT NULL,
  end_device_id String NOT NULL,
  occurred_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data JSON NOT NULL
) ENGINE = MergeTree()
PRIMARY KEY (organization_id, occurred_at, end_device_id)
ORDER BY (organization_id, occurred_at, end_device_id)
PARTITION BY toYYYYMM(occurred_at)
SETTINGS index_granularity = 8192;
-- +goose StatementEnd
```

### Dependencies (Cargo.toml)
```toml
[dependencies]
clickhouse = { version = "0.14", features = ["lz4", "inserter", "chrono"] }
tokio = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["clickhouse"] }
which = "7.0"
```

### Error Observed
When running integration tests with the above code:
```
While processing column ProcessedEnvelopeRow.occurred_at:
attempting to (de)serialize ClickHouse type DateTime as &str
which is not compatible
```

This error suggests that without the serde attributes, the clickhouse-rs library is treating DateTime columns as strings when JSON type is present in the schema.

---

## Key Files

- **Integration Tests**: `crates/ponix-clickhouse/tests/integration.rs`
- **Migrations**: `crates/ponix-clickhouse/migrations/`
- **Client**: `crates/ponix-clickhouse/src/client.rs`
- **Envelope Store**: `crates/ponix-clickhouse/src/envelope_store.rs`
- **Migration Runner**: `crates/ponix-clickhouse/src/migration.rs`

---

## Test Results Summary

| Test Name | clickhouse 0.13 + String | clickhouse 0.14 + JSON |
|-----------|--------------------------|------------------------|
| `test_clickhouse_connection` | ✅ PASS | ✅ PASS |
| `test_empty_batch` | ✅ PASS | ✅ PASS |
| `test_batch_write_with_migrations` | ❌ Binary format error | ❌ DateTime type mismatch |
| `test_large_batch_write` | ❌ Binary format error | ❌ DateTime type mismatch |
| `test_envelope_with_data` | ❌ Binary format error | ❌ DateTime type mismatch |

---

## Conclusion

The ClickHouse Rust client currently has compatibility issues when:
1. Using binary format for batch inserts with clickhouse-rs 0.13 (regardless of data types)
2. Using JSON type alongside DateTime columns with clickhouse-rs 0.14

**Recommended Path Forward**: Explore HTTP-based insert formats (JSONEachRow) as an alternative to binary format, which may provide better compatibility with ClickHouse 24.10+ and the JSON type.

---

## UPDATE: Integration Test Setup Success ✅

### Working Configuration Found

After extensive research and testing, we successfully configured testcontainers with ClickHouse:

**Key Discoveries**:
1. **clickhouse-rs uses HTTP protocol** - The library connects via `http://host:port`, NOT TCP
2. **Port 8123 is correct** - This is the HTTP interface port (NOT port 9000 which is native TCP)
3. **testcontainers-modules has ClickHouse support** - No need for GenericImage
4. **Custom Image wrapper needed for v24.10** - Default testcontainers uses 23.3.8.21

**Working Test Code**:
```rust
use ponix_clickhouse::ClickHouseClient;
use testcontainers::runners::AsyncRunner;
use testcontainers::Image;
use testcontainers_modules::clickhouse::ClickHouse;

/// Custom ClickHouse image with version 24.10 for JSON type support
#[derive(Debug, Clone)]
struct ClickHouse24 {
    inner: ClickHouse,
}

impl Default for ClickHouse24 {
    fn default() -> Self {
        Self {
            inner: ClickHouse::default(),
        }
    }
}

impl Image for ClickHouse24 {
    fn name(&self) -> &str {
        "clickhouse/clickhouse-server"
    }

    fn tag(&self) -> &str {
        "24.10"
    }

    fn ready_conditions(&self) -> Vec<testcontainers::core::WaitFor> {
        self.inner.ready_conditions()
    }

    fn env_vars(&self) -> impl IntoIterator<...> {
        self.inner.env_vars()
    }

    fn expose_ports(&self) -> &[testcontainers::core::ContainerPort] {
        self.inner.expose_ports()
    }
}

#[tokio::test]
async fn test_clickhouse_connection() {
    let clickhouse = ClickHouse24::default().start().await.unwrap();
    let host = clickhouse.get_host().await.unwrap();
    let port = clickhouse.get_host_port_ipv4(8123).await.unwrap();

    let client = ClickHouseClient::new(
        &format!("http://{}:{}", host, port),
        "default",
        "default",
        "",
    );

    client.ping().await.expect("Should be able to ping ClickHouse");
}
```

**Test Status**: ✅ **PASSING**

### Common Mistakes to Avoid
- ❌ Using `tcp://host:port` - clickhouse-rs requires HTTP scheme
- ❌ Using `localhost:port` without scheme - must include `http://`
- ❌ Using port 9000 - that's the native TCP protocol, not HTTP
- ❌ Using GenericImage directly - testcontainers-modules has better ClickHouse support
- ✅ Use `http://host:port` format
- ✅ Use port 8123 (HTTP interface)
- ✅ Create custom Image wrapper for version 24.10
- ✅ Let testcontainers handle ready conditions
