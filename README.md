# Ponix

An IoT data insights platform that transforms raw device telemetry into actionable data.

## What is Ponix?

Ponix is designed to solve a common IoT challenge: devices send raw binary payloads, but you need structured, queryable data. Ponix sits between your IoT devices and your analytics tools, automatically transforming device payloads into clean JSON that's ready for analysis.

**The Problem:**
- Your temperature sensor sends `[0x01, 0x67, 0x01, 0x10]`
- You need `{"temperature": 27.2, "unit": "celsius"}`

**The Solution:**
Ponix uses CEL (Common Expression Language) to transform device payloads on-the-fly, storing the results for real-time queries and historical analysis.

## Key Features

### 🔧 Flexible Payload Transformation
Configure each device with custom CEL expressions to transform binary data into structured JSON. Built-in support for Cayenne LPP and extensible for custom formats.

### 📊 Time-Series Analytics
Automatic storage in ClickHouse provides fast queries across millions of data points with full-text search and aggregations.

### 🎯 Device Management
Simple gRPC API to register devices, configure transformations, and manage your IoT fleet.

### ⚡ Real-Time Processing
Event-driven architecture with NATS JetStream ensures low-latency data processing and reliable delivery.

## Quick Start

### Prerequisites
- Docker and Docker Compose
- (Optional) [mise](https://mise.jdx.dev/) for task management

### Run Ponix

```bash
# Start all services (PostgreSQL, ClickHouse, NATS, and Ponix)
docker-compose -f docker/docker-compose.deps.yaml up -d
docker-compose -f docker/docker-compose.service.yaml up --build

# Or use Tilt for development
tilt up
```

**Services will be available at:**
- gRPC API: `localhost:50051`
- NATS: `localhost:4222`
- ClickHouse: `localhost:8123` (HTTP), `localhost:9000` (native)
- PostgreSQL: `localhost:5432`

## Usage Example

### 1. Register a Device

```bash
# Install grpcurl
brew install grpcurl

# Create a temperature sensor with Cayenne LPP payload conversion
grpcurl -plaintext -d '{
  "organization_id": "my-org",
  "name": "Temperature Sensor",
  "payload_conversion": "cayenne_lpp_decode(input)"
}' localhost:50051 ponix.end_device.v1.EndDeviceService/CreateEndDevice
```

### 2. Send Data

When your device sends a Cayenne LPP payload like `[0x01, 0x67, 0x01, 0x10]`, Ponix automatically:
1. Looks up the device configuration
2. Executes the CEL expression: `cayenne_lpp_decode(input)`
3. Transforms to: `{"temperature_1": 27.2}`
4. Stores in ClickHouse for querying

### 3. Query Your Data

```bash
# Connect to ClickHouse
docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix

# Query recent sensor data
SELECT
  end_device_id,
  occurred_at,
  data
FROM ponix.processed_envelopes
WHERE end_device_id = 'your-device-id'
ORDER BY occurred_at DESC
LIMIT 10;
```

## Advanced Usage

### Custom Transformations

Create sophisticated transformations with CEL:

```javascript
// Unit conversion and enrichment
{
  'temp_c': cayenne_lpp_decode(input).temperature_1,
  'temp_f': cayenne_lpp_decode(input).temperature_1 * 9.0 / 5.0 + 32.0,
  'humidity': cayenne_lpp_decode(input).humidity_2,
  'timestamp': timestamp(now)
}
```

### Conditional Logic

```javascript
// Alert on high temperature
cayenne_lpp_decode(input).temperature_1 > 30 ?
  {'status': 'alert', 'message': 'High temperature detected'} :
  {'status': 'normal', 'temp': cayenne_lpp_decode(input).temperature_1}
```

### Device Management

```bash
# List all devices for an organization
grpcurl -plaintext -d '{
  "organization_id": "my-org"
}' localhost:50051 ponix.end_device.v1.EndDeviceService/ListEndDevices

# Get device details
grpcurl -plaintext -d '{
  "device_id": "device-id-here"
}' localhost:50051 ponix.end_device.v1.EndDeviceService/GetEndDevice
```

## How It Works

```
IoT Device → Raw Payload → Ponix → Structured Data → Analytics
             [0x01...]      ↓      {"temp": 27.2}
                            CEL
                        Transformation
```

1. **Device Registration**: Configure each device with a CEL expression
2. **Data Ingestion**: Raw binary payloads arrive via NATS
3. **Transformation**: CEL engine converts binary to JSON based on device config
4. **Storage**: Structured data is stored in ClickHouse
5. **Query**: Run SQL queries on your IoT data

## Built-In Functions

- `cayenne_lpp_decode(input)` - Decode Cayenne Low Power Payload format
- `base64_encode(bytes)` - Encode bytes to base64
- Standard CEL operators and functions

## Development

### Running Tests

```bash
# Unit tests (fast, no Docker required)
cargo test --workspace --lib --bins

# Integration tests (requires Docker)
cargo test --workspace --features integration-tests -- --test-threads=1
```

### Project Structure

Ponix is built as a Rust workspace with clean separation of concerns:
- **Domain layer** - Business logic and data transformations
- **Infrastructure** - Database clients, message queues
- **API** - gRPC service handlers

See [CLAUDE.md](CLAUDE.md) for detailed development documentation.

## Configuration

Configure via environment variables:

```bash
# Database connections
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
CLICKHOUSE_URL=http://localhost:8123
NATS_URL=nats://localhost:4222

# gRPC API
PONIX_GRPC_HOST=0.0.0.0
PONIX_GRPC_PORT=50051
```

## License

MIT
