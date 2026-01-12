#!/bin/bash
set -euo pipefail

# E2E Test: MQTT Gateway Pipeline
# Tests the complete flow: MQTT -> Gateway -> NATS -> Analytics Worker -> ClickHouse
#
# Prerequisites:
#   - mqttx-cli (mise install)
#   - grpcurl and jq installed
#   - ponix service running (tilt up or docker-compose)
#
# Usage:
#   ./scripts/test-mqtt-pipeline.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

# Additional MQTT configuration
MQTT_HOST="${PONIX_MQTT_HOST:-localhost}"
MQTT_PORT="${PONIX_MQTT_PORT:-1883}"
GATEWAY_BROKER_URL="${PONIX_GATEWAY_BROKER_URL:-mqtt://ponix-emqx:1883}"

init_test_script "MQTT Pipeline E2E Test"

echo -e "${YELLOW}MQTT Configuration:${NC}"
echo -e "  MQTT broker: ${MQTT_HOST}:${MQTT_PORT}"
echo -e "  Gateway URL: ${GATEWAY_BROKER_URL}"
echo ""

# Check MQTT prerequisites
require_command "mqttx-cli" "mise install"

# ============================================
# SETUP - Create all required resources
# ============================================

print_step "Setup: Authenticating..."
setup_test_auth

if [ -z "$AUTH_TOKEN" ] || [ "$AUTH_TOKEN" = "null" ]; then
    print_error "Failed to authenticate"
    exit 1
fi
print_success "Authenticated as $TEST_EMAIL"

# Create Organization
print_step "Setup: Creating organization..."
ORG_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/CreateOrganization" \
    '{"name": "MQTT Pipeline Test Org"}')

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -z "$ORG_ID" ] || [ "$ORG_ID" = "null" ]; then
    print_error "Failed to create organization"
    echo "$ORG_RESPONSE"
    exit 1
fi
print_success "Organization: $ORG_ID"

# Create Workspace
print_step "Setup: Creating workspace..."
WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"MQTT Test Workspace\"}")

WORKSPACE_ID=$(echo "$WORKSPACE_RESPONSE" | jq -r '.workspace.id // .id // empty')
if [ -z "$WORKSPACE_ID" ] || [ "$WORKSPACE_ID" = "null" ]; then
    print_error "Failed to create workspace"
    echo "$WORKSPACE_RESPONSE"
    exit 1
fi
print_success "Workspace: $WORKSPACE_ID"

# Create Gateway (will trigger CDC event and start MQTT subscriber)
print_step "Setup: Creating MQTT gateway..."
GATEWAY_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/CreateGateway" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Test EMQX Gateway\",
        \"type\": \"GATEWAY_TYPE_EMQX\",
        \"emqx_config\": {
            \"broker_url\": \"$GATEWAY_BROKER_URL\"
        }
    }")

GATEWAY_ID=$(echo "$GATEWAY_RESPONSE" | jq -r '.gateway.gatewayId // .gatewayId // empty')
if [ -z "$GATEWAY_ID" ] || [ "$GATEWAY_ID" = "null" ]; then
    print_error "Failed to create gateway"
    echo "$GATEWAY_RESPONSE"
    exit 1
fi
print_success "Gateway: $GATEWAY_ID"

# Create End Device Definition with Cayenne LPP conversion
print_step "Setup: Creating device definition with Cayenne LPP converter..."
DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Cayenne LPP Temperature Sensor\",
        \"json_schema\": \"{}\",
        \"payload_conversion\": \"cayenne_lpp_decode(input)\"
    }")

DEFINITION_ID=$(echo "$DEFINITION_RESPONSE" | jq -r '.endDeviceDefinition.id // .id // empty')
if [ -z "$DEFINITION_ID" ] || [ "$DEFINITION_ID" = "null" ]; then
    print_error "Failed to create definition"
    echo "$DEFINITION_RESPONSE"
    exit 1
fi
print_success "Definition: $DEFINITION_ID"

# Create End Device
print_step "Setup: Creating end device..."
DEVICE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceService/CreateEndDevice" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"gateway_id\": \"$GATEWAY_ID\",
        \"name\": \"Temperature Sensor\"
    }")

DEVICE_ID=$(echo "$DEVICE_RESPONSE" | jq -r '.endDevice.deviceId // .deviceId // empty')
if [ -z "$DEVICE_ID" ] || [ "$DEVICE_ID" = "null" ]; then
    print_error "Failed to create device"
    echo "$DEVICE_RESPONSE"
    exit 1
fi
print_success "Device: $DEVICE_ID"

# ============================================
# WAIT FOR GATEWAY CONNECTION
# ============================================

print_step "Waiting for gateway to connect to MQTT broker..."
sleep 3
print_success "Gateway connection window elapsed"

# ============================================
# MQTT PUBLISH TESTS
# ============================================

# Cayenne LPP format: [channel] [type] [data...]
# Temperature (type 0x67): 2 bytes, 0.1C resolution, signed
# Humidity (type 0x68): 1 byte, 0.5% resolution

print_step "Publishing Cayenne LPP message via MQTT..."
echo -e "  Topic: ${ORG_ID}/${DEVICE_ID}"

# Temperature: 25.5C = 255 = 0x00FF, Humidity: 50% = 100 = 0x64
CAYENNE_PAYLOAD_HEX="016700FF026864"
echo -e "  Payload (hex): ${CAYENNE_PAYLOAD_HEX}"
echo -e "  Decoded: Temperature 25.5C, Humidity 50%"

mqttx-cli pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -m "${CAYENNE_PAYLOAD_HEX}" \
    --format hex

print_success "Message 1 published"

# Publish additional messages
print_step "Publishing additional test messages..."

# Temperature: 30.0C = 300 = 0x012C, Humidity: 70% = 140 = 0x8C
mqttx-cli pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -m "0167012C02688C" \
    --format hex
print_info "Published: Temperature 30.0C, Humidity 70%"

sleep 1

# Temperature: 18.5C = 185 = 0x00B9, Humidity: 45% = 90 = 0x5A
mqttx-cli pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -m "016700B902685A" \
    --format hex
print_info "Published: Temperature 18.5C, Humidity 45%"

print_success "All messages published"

# ============================================
# SUMMARY
# ============================================

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}MQTT Pipeline Test Complete${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo -e "${YELLOW}Resources Created:${NC}"
echo -e "  Organization: $ORG_ID"
echo -e "  Workspace:    $WORKSPACE_ID"
echo -e "  Gateway:      $GATEWAY_ID"
echo -e "  Definition:   $DEFINITION_ID"
echo -e "  Device:       $DEVICE_ID"
echo ""
echo -e "${YELLOW}Expected Pipeline Flow:${NC}"
echo -e "  1. MQTT message received by gateway"
echo -e "  2. Published to NATS as RawEnvelope"
echo -e "  3. Processed by analytics worker (Cayenne LPP decoded)"
echo -e "  4. Stored in ClickHouse as ProcessedEnvelope"
echo ""
echo -e "${YELLOW}Verification Commands:${NC}"
echo ""
echo -e "${BLUE}Check service logs:${NC}"
echo "  docker logs ponix-all-in-one 2>&1 | grep $DEVICE_ID"
echo ""
echo -e "${BLUE}Query ClickHouse:${NC}"
echo "  docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix \\"
echo "    -q \"SELECT * FROM ponix.processed_envelopes WHERE end_device_id = '$DEVICE_ID'\""
echo ""
echo -e "${BLUE}View traces in Grafana:${NC}"
echo "  http://localhost:3000"
