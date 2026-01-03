# Test Scripts

This directory contains test scripts for the Ponix gRPC API, organized by domain area.

## Quick Start

```bash
# Run all tests
./scripts/test-all.sh

# Run individual test suites
./scripts/test-auth.sh
./scripts/test-organizations.sh
./scripts/test-workspaces.sh
./scripts/test-devices.sh
./scripts/test-gateways.sh
./scripts/test-mqtt-pipeline.sh
```

## Prerequisites

- `mise install`
- `tilt up`

## Test Scripts

| Script | Service(s) | Description |
|--------|------------|-------------|
| `test-all.sh` | All | Runner script - executes all tests in sequence |
| `test-auth.sh` | UserService | Authentication flow (register, login, refresh, logout, GetUser) |
| `test-organizations.sh` | OrganizationService | Organization CRUD operations |
| `test-workspaces.sh` | WorkspaceService | Workspace CRUD operations |
| `test-devices.sh` | EndDeviceService, EndDeviceDefinitionService | Device and definition CRUD |
| `test-gateways.sh` | GatewayService | Gateway CRUD operations |
| `test-mqtt-pipeline.sh` | E2E | MQTT -> NATS -> ClickHouse pipeline test |

## Directory Structure

```
scripts/
├── README.md              # This file
├── lib/
│   └── common.sh          # Shared utilities (colors, auth helpers, test functions)
├── test-all.sh            # Runner script
├── test-auth.sh           # UserService tests
├── test-organizations.sh  # OrganizationService tests
├── test-workspaces.sh     # WorkspaceService tests
├── test-devices.sh        # EndDevice + Definition tests
├── test-gateways.sh       # GatewayService tests
└── test-mqtt-pipeline.sh  # E2E MQTT pipeline test
```

## Test Coverage

Each domain test script includes:
- **Happy path tests**: CRUD operations with valid authentication
- **UNAUTHENTICATED tests**: Verify all protected endpoints reject requests without tokens
- **Invalid token tests**: Verify endpoints reject invalid JWT tokens

### API Coverage (28 RPC methods)

| Service | Methods | Script |
|---------|---------|--------|
| UserService | RegisterUser, Login, Refresh, Logout, GetUser | test-auth.sh |
| OrganizationService | Create, Get, Delete, List, UserOrganizations | test-organizations.sh |
| WorkspaceService | Create, Get, Update, Delete, List | test-workspaces.sh |
| EndDeviceDefinitionService | Create, Get, Update, Delete, List | test-devices.sh |
| EndDeviceService | Create, Get, List | test-devices.sh |
| GatewayService | Create, Get, Update, Delete, List | test-gateways.sh |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GRPC_HOST` | `localhost:50051` | gRPC server address |
| `PONIX_MQTT_HOST` | `localhost` | MQTT broker host |
| `PONIX_MQTT_PORT` | `1883` | MQTT broker port |
| `PONIX_GATEWAY_BROKER_URL` | `mqtt://ponix-emqx:1883` | Gateway broker URL (Docker network) |

Example:
```bash
GRPC_HOST=192.168.1.100:50051 ./scripts/test-all.sh
```

## Writing New Tests

Source the common library and follow the established pattern:

```bash
#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "My Service Tests"

# Setup
print_step "Setup: Authenticating..."
AUTH_TOKEN=$(register_and_login)
print_success "Authenticated as $TEST_EMAIL"

# Happy path tests
print_step "Testing MyMethod (happy path)..."
RESPONSE=$(grpc_call "$AUTH_TOKEN" "my.v1.MyService/MyMethod" '{"field": "value"}')
# ... validate response ...
print_success "MyMethod works correctly"

# Unauthenticated tests
print_step "Testing MyMethod without auth..."
test_unauthenticated "my.v1.MyService/MyMethod" '{"field": "value"}'

# Summary
print_summary "My Service Tests"
```

### Available Helper Functions

From `lib/common.sh`:

| Function | Description |
|----------|-------------|
| `init_test_script "Name"` | Initialize script with header and prerequisites check |
| `print_step "message"` | Print step header (green) |
| `print_success "message"` | Print success message |
| `print_error "message"` | Print error message |
| `print_warning "message"` | Print warning message |
| `print_info "message"` | Print info message |
| `register_and_login` | Register new user and return JWT token |
| `grpc_call "$token" "method" "$payload"` | Make authenticated gRPC call |
| `test_unauthenticated "method" "$payload"` | Verify endpoint rejects unauthenticated requests |
| `test_invalid_token "method" "$payload"` | Verify endpoint rejects invalid tokens |
| `print_summary "Name"` | Print test summary footer |

## Verifying Test Results

### Check Service Logs
```bash
docker logs ponix-all-in-one 2>&1 | grep <device_id>
```

### Query ClickHouse
```bash
docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix \
  -q "SELECT * FROM ponix.processed_envelopes ORDER BY received_at DESC LIMIT 10;"
```

### Verify RBAC Assignments
```bash
docker exec -it ponix-postgres psql -U ponix -d ponix -c \
  "SELECT * FROM casbin_rule WHERE ptype = 'g';"
```

### Subscribe to CDC Events
```bash
nats sub 'gateway.>'
```

## gRPC Reflection

gRPC reflection is enabled, allowing grpcurl to discover services:

```bash
# List all services
grpcurl -plaintext localhost:50051 list

# List methods for a service
grpcurl -plaintext localhost:50051 list organization.v1.OrganizationService

# Describe a method
grpcurl -plaintext localhost:50051 describe organization.v1.OrganizationService.CreateOrganization
```
