#!/bin/bash
set -euo pipefail

# E2E Test: Multi-Contract Pipeline
# Tests that a single device with two payload contracts correctly routes
# messages based on payload size using first-match-wins semantics.
#
# Contract 1: size(input) == 4 → temp-only (4-byte Cayenne LPP)
# Contract 2: size(input) == 7 → temp+humidity (7-byte Cayenne LPP)
#
# Prerequisites:
#   - mqttx-cli (mise install)
#   - grpcurl and jq installed
#   - ponix service running (tilt up or docker-compose)
#
# Usage:
#   ./scripts/test-multi-contract-pipeline.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

# Additional configuration
MQTT_HOST="${PONIX_MQTT_HOST:-localhost}"
MQTT_PORT="${PONIX_MQTT_PORT:-1883}"
GATEWAY_BROKER_URL="${PONIX_GATEWAY_BROKER_URL:-mqtt://ponix-emqx:1883}"
CLICKHOUSE_CONTAINER="${PONIX_CLICKHOUSE_CONTAINER:-ponix-clickhouse}"
CLICKHOUSE_USER="${PONIX_CLICKHOUSE_USERNAME:-ponix}"
CLICKHOUSE_PASSWORD="${PONIX_CLICKHOUSE_PASSWORD:-ponix}"

init_test_script "Multi-Contract Pipeline E2E Test"

echo -e "${YELLOW}MQTT Configuration:${NC}"
echo -e "  MQTT broker: ${MQTT_HOST}:${MQTT_PORT}"
echo -e "  Gateway URL: ${GATEWAY_BROKER_URL}"
echo ""

# Check prerequisites
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
    '{"name": "Multi-Contract Test Org"}')

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
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Multi-Contract Test Workspace\"}")

WORKSPACE_ID=$(echo "$WORKSPACE_RESPONSE" | jq -r '.workspace.id // .id // empty')
if [ -z "$WORKSPACE_ID" ] || [ "$WORKSPACE_ID" = "null" ]; then
    print_error "Failed to create workspace"
    echo "$WORKSPACE_RESPONSE"
    exit 1
fi
print_success "Workspace: $WORKSPACE_ID"

# Create Gateway
print_step "Setup: Creating MQTT gateway..."
GATEWAY_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/CreateGateway" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Multi-Contract Test Gateway\",
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

# Create End Device Definition with TWO contracts
print_step "Setup: Creating device definition with two contracts..."
DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Multi-Contract Sensor\",
        \"contracts\": [
            {
                \"match_expression\": \"size(input) == 4\",
                \"transform_expression\": \"cayenne_lpp_decode(input)\",
                \"json_schema\": \"{}\"
            },
            {
                \"match_expression\": \"size(input) == 7\",
                \"transform_expression\": \"cayenne_lpp_decode(input)\",
                \"json_schema\": \"{}\"
            }
        ]
    }")

DEFINITION_ID=$(echo "$DEFINITION_RESPONSE" | jq -r '.endDeviceDefinition.id // .id // empty')
if [ -z "$DEFINITION_ID" ] || [ "$DEFINITION_ID" = "null" ]; then
    print_error "Failed to create definition"
    echo "$DEFINITION_RESPONSE"
    exit 1
fi
print_success "Definition: $DEFINITION_ID (2 contracts)"

# Create End Device
print_step "Setup: Creating end device..."
DEVICE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceService/CreateEndDevice" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"gateway_id\": \"$GATEWAY_ID\",
        \"name\": \"Multi-Contract Sensor\"
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
sleep 5

# Send a warmup message to ensure the gateway subscription is active.
# The CDC pipeline (postgres -> nats -> gateway orchestrator -> MQTT subscribe)
# can take a few seconds; this sacrificial publish absorbs any remaining lag.
mqttx-cli pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -m "00" \
    --format hex
sleep 2
print_success "Gateway connection window elapsed"

# ============================================
# PUBLISH TWO DIFFERENT MESSAGE TYPES
# ============================================

# Message type 1: temp-only (4 bytes)
# Cayenne LPP: channel=01, type=67 (temp), value=00FF (25.5°C)
PAYLOAD_TEMP_ONLY="016700FF"

print_step "Publishing message type 1: temp-only (4 bytes)..."
echo -e "  Topic: ${ORG_ID}/${DEVICE_ID}"
echo -e "  Payload (hex): ${PAYLOAD_TEMP_ONLY}"
echo -e "  Expected match: contract 1 (size(input) == 4)"
echo -e "  Decoded: Temperature 25.5°C"

mqttx-cli pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -m "${PAYLOAD_TEMP_ONLY}" \
    --format hex

print_success "Message type 1 published (temp-only)"

sleep 2

# Message type 2: temp+humidity (7 bytes)
# Cayenne LPP: channel=01 type=67 value=00FF (25.5°C) + channel=02 type=68 value=64 (50%)
PAYLOAD_TEMP_HUMIDITY="016700FF026864"

print_step "Publishing message type 2: temp+humidity (7 bytes)..."
echo -e "  Topic: ${ORG_ID}/${DEVICE_ID}"
echo -e "  Payload (hex): ${PAYLOAD_TEMP_HUMIDITY}"
echo -e "  Expected match: contract 2 (size(input) == 7)"
echo -e "  Decoded: Temperature 25.5°C, Humidity 50%"

mqttx-cli pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -m "${PAYLOAD_TEMP_HUMIDITY}" \
    --format hex

print_success "Message type 2 published (temp+humidity)"

# ============================================
# VERIFICATION - Query ClickHouse
# ============================================

print_step "Waiting for pipeline processing..."
sleep 5

print_step "Querying ClickHouse for processed envelopes..."

QUERY="SELECT data FROM ponix.processed_envelopes WHERE end_device_id = '$DEVICE_ID' ORDER BY received_at FORMAT JSONEachRow"

# Poll for results — the pipeline may take a few seconds to flush to ClickHouse
MAX_ATTEMPTS=6
ATTEMPT=0
ROW_COUNT=0
CH_RESULT=""

while [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]; do
    ((ATTEMPT++))
    CH_RESULT=$(docker exec "$CLICKHOUSE_CONTAINER" clickhouse-client \
        -u "$CLICKHOUSE_USER" --password "$CLICKHOUSE_PASSWORD" \
        -q "$QUERY" 2>&1) || true

    if [ -n "$CH_RESULT" ]; then
        ROW_COUNT=$(echo "$CH_RESULT" | wc -l | tr -d ' ')
        if [ "$ROW_COUNT" -ge 2 ]; then
            break
        fi
    fi

    if [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]; then
        print_info "Attempt $ATTEMPT/$MAX_ATTEMPTS: found $ROW_COUNT rows, waiting..."
        sleep 3
    fi
done

if [ "$ROW_COUNT" -lt 2 ]; then
    print_error "Expected 2 processed envelopes, found $ROW_COUNT after $MAX_ATTEMPTS attempts"
    echo -e "${YELLOW}Results:${NC}"
    echo "$CH_RESULT"
    echo -e "${YELLOW}Debug: Check service logs with:${NC}"
    echo "  docker logs ponix-all-in-one 2>&1 | grep $DEVICE_ID"
    exit 1
fi

print_success "Found $ROW_COUNT processed envelopes"

# Verify first row (temp-only) has temperature but no humidity
FIRST_DATA=$(echo "$CH_RESULT" | head -1 | jq -r '.data')
print_info "Row 1 data: $FIRST_DATA"

if echo "$FIRST_DATA" | jq -e '.temperature_1' > /dev/null 2>&1; then
    print_success "Row 1 has temperature data"
else
    print_error "Row 1 missing temperature data"
    exit 1
fi

if echo "$FIRST_DATA" | jq -e '.humidity_2' > /dev/null 2>&1; then
    print_error "Row 1 should NOT have humidity data (temp-only contract)"
    exit 1
else
    print_success "Row 1 correctly has no humidity data"
fi

# Verify second row (temp+humidity) has both
SECOND_DATA=$(echo "$CH_RESULT" | head -2 | tail -1 | jq -r '.data')
print_info "Row 2 data: $SECOND_DATA"

if echo "$SECOND_DATA" | jq -e '.temperature_1' > /dev/null 2>&1; then
    print_success "Row 2 has temperature data"
else
    print_error "Row 2 missing temperature data"
    exit 1
fi

if echo "$SECOND_DATA" | jq -e '.humidity_2' > /dev/null 2>&1; then
    print_success "Row 2 has humidity data"
else
    print_error "Row 2 missing humidity data (should have temp+humidity)"
    exit 1
fi

# ============================================
# SUMMARY
# ============================================

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Multi-Contract Pipeline Test PASSED${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo -e "${YELLOW}Verified:${NC}"
echo -e "  - Contract 1 (size==4) matched temp-only payload → temperature data only"
echo -e "  - Contract 2 (size==7) matched temp+humidity payload → both fields present"
echo -e "  - First-match-wins routing works correctly"
echo ""
echo -e "${YELLOW}Resources Created:${NC}"
echo -e "  Organization: $ORG_ID"
echo -e "  Workspace:    $WORKSPACE_ID"
echo -e "  Gateway:      $GATEWAY_ID"
echo -e "  Definition:   $DEFINITION_ID (2 contracts)"
echo -e "  Device:       $DEVICE_ID"
