# Test Scripts

This directory contains test scripts for the Ponix gRPC API and MQTT gateway.

| Script | Purpose |
|--------|---------|
| `test-grpc-api.sh` | Comprehensive gRPC API test (CRUD operations, auth) |
| `test_mqtt_gateway.sh` | End-to-end MQTT pipeline test (MQTT → NATS → ClickHouse) |
| `test-login.sh` | Authentication flow test (login, refresh, logout) |

## test-grpc-api.sh

A comprehensive test script for the Ponix gRPC API that creates an organization, end device, and gateway.

### Prerequisites

- `grpcurl` CLI tool: `brew install grpcurl`
- `jq` for JSON formatting: `brew install jq`
- Running Ponix service with gRPC server on `localhost:50051`

### What It Does

1. **Creates an Organization** with test metadata
2. **Creates a Workspace** within the organization (required for devices)
3. **Creates an End Device Definition** with Cayenne LPP payload converter
   - Payload converter: `cayenne_lpp.decode(payload)`
   - Automatically decodes Cayenne LPP binary payloads to JSON
4. **Creates an End Device** referencing the definition and workspace
5. **Creates a Gateway** (EMQX type) with connection configuration
6. **Verifies** all created resources by listing and getting details
7. **Tests Updates** by updating the gateway configuration (triggers CDC event)

### Usage

```bash
# Start infrastructure
docker-compose -f docker/docker-compose.deps.yaml up -d

# Start the service
cargo run -p ponix-all-in-one

# In another terminal, run the test script
cd scripts
./test-grpc-api.sh
```

### Output

The script will output:
- Colored, formatted JSON responses for each operation
- All created resource IDs (Organization, Workspace, Definition, Device, Gateway)
- Next steps for testing CDC and payload processing

### Testing CDC Events

After running the script, you can verify CDC events are being published to NATS:

```bash
# Subscribe to all gateway CDC events
nats sub 'gateway.>'

# Then update a gateway (either through the script or manually)
# You should see gateway.update events published with protobuf payloads
```

### Verifying gRPC Reflection

gRPC reflection is enabled on the server, allowing grpcurl to discover services automatically:

```bash
# List all services
grpcurl -plaintext localhost:50051 list

# List methods for a specific service
grpcurl -plaintext localhost:50051 list ponix.gateway.v1.GatewayService

# Describe a specific method
grpcurl -plaintext localhost:50051 describe ponix.gateway.v1.GatewayService.CreateGateway
```

### Environment Variables

- `GRPC_HOST`: Override the default gRPC server address (default: `localhost:50051`)

Example:
```bash
GRPC_HOST=192.168.1.100:50051 ./test-grpc-api.sh
```

## Manual Testing Examples

### Authentication Setup

First, register and login to get a JWT token:

```bash
# Register user
grpcurl -plaintext \
    -d '{
        "email": "test@example.com",
        "password": "password123",
        "name": "Test User"
    }' \
    localhost:50051 user.v1.UserService/RegisterUser

# Login to get JWT token
LOGIN_RESPONSE=$(grpcurl -plaintext \
    -d '{
        "email": "test@example.com",
        "password": "password123"
    }' \
    localhost:50051 user.v1.UserService/Login)

# Extract token
AUTH_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token')
echo "Token: $AUTH_TOKEN"
```

### Create Organization

Creating an organization automatically assigns the creator as Admin (RBAC role).

```bash
ORG_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d '{
        "name": "My Test Org"
    }' \
    localhost:50051 organization.v1.OrganizationService/CreateOrganization)

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId')
echo "Organization ID: $ORG_ID"
```

### Create Workspace

Workspaces are required to group devices within an organization.

```bash
WORKSPACE_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Default Workspace\"
    }" \
    localhost:50051 workspace.v1.WorkspaceService/CreateWorkspace)

WORKSPACE_ID=$(echo "$WORKSPACE_RESPONSE" | jq -r '.workspace.id')
echo "Workspace ID: $WORKSPACE_ID"
```

### Create End Device Definition

```bash
DEFINITION_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Cayenne LPP Sensor\",
        \"json_schema\": \"{}\",
        \"payload_conversion\": \"cayenne_lpp_decode(input)\"
    }" \
    localhost:50051 end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition)

DEFINITION_ID=$(echo "$DEFINITION_RESPONSE" | jq -r '.endDeviceDefinition.id')
echo "Definition ID: $DEFINITION_ID"
```

### Create End Device

Devices require a workspace_id and definition_id.

```bash
grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"name\": \"Temperature Sensor\"
    }" \
    localhost:50051 end_device.v1.EndDeviceService/CreateEndDevice
```

### Create Gateway

```bash
grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d '{
        "organization_id": "org-xxx",
        "name": "EMQX Gateway",
        "type": "GATEWAY_TYPE_EMQX",
        "emqx_config": {
            "broker_url": "mqtt://mqtt.example.com:1883",
            "subscription_group": "ponix"
        }
    }' \
    localhost:50051 gateway.v1.GatewayService/CreateGateway
```

### Update Gateway (Triggers CDC Event)

```bash
grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d '{
        "gateway_id": "gw-xxx",
        "organization_id": "org-xxx",
        "name": "EMQX Gateway - Updated"
    }' \
    localhost:50051 gateway.v1.GatewayService/UpdateGateway
```

### Verify RBAC Role Assignment

After creating an organization, verify the creator was assigned the Admin role:

```bash
docker exec -it ponix-postgres psql -U ponix -d ponix -c \
    "SELECT * FROM casbin_rule WHERE ptype = 'g';"
```

This should show a row with:
- `ptype = 'g'` (grouping policy)
- `v0 = <user_id>` (user who created the org)
- `v1 = 'admin'` (role)
- `v2 = <org_id>` (organization domain)

## Next Steps

After creating resources:

1. **Subscribe to NATS for CDC events**:
   ```bash
   nats sub 'gateway.>'
   ```

2. **Send a raw envelope** (if you created a device with Cayenne converter):
   ```bash
   # The device will decode the Cayenne LPP payload to JSON
   # and publish to processed_envelopes stream
   ```

3. **Query ClickHouse** for processed envelopes:
   ```bash
   docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix
   SELECT * FROM ponix.processed_envelopes ORDER BY received_at DESC LIMIT 10;
   ```

4. **Check CDC replication slot status**:
   ```bash
   docker exec -it ponix-postgres psql -U ponix -d ponix -c \
     "SELECT * FROM pg_replication_slots WHERE slot_name = 'ponix_cdc_slot';"
   ```
