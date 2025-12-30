#!/bin/bash

# Test script for MQTT Gateway functionality
# Creates a gateway, device, and publishes a Cayenne LPP message
#
# Prerequisites:
#   - mosquitto_pub (mise run install-tools)
#   - mise with grpcurl and jq installed (mise install)
#   - ponix service running (tilt up or docker-compose)
#
# Usage:
#   ./scripts/test_mqtt_gateway.sh

set -euo pipefail

# Configuration
GRPC_HOST="${PONIX_GRPC_HOST:-localhost}"
GRPC_PORT="${PONIX_GRPC_PORT:-50051}"
MQTT_HOST="${PONIX_MQTT_HOST:-localhost}"
MQTT_PORT="${PONIX_MQTT_PORT:-1883}"

# For gateway config, use docker network name when service runs in docker
# Use localhost when running natively
GATEWAY_BROKER_URL="${PONIX_GATEWAY_BROKER_URL:-mqtt://ponix-emqx:1883}"

# Test data
DEFINITION_NAME="Cayenne LPP Temperature Sensor"
DEVICE_NAME="Temperature Sensor"
PAYLOAD_CONVERSION="cayenne_lpp_decode(input)"
JSON_SCHEMA="{}"

echo "=== MQTT Gateway Test Script ==="
echo ""
echo "Configuration:"
echo "  gRPC endpoint: ${GRPC_HOST}:${GRPC_PORT}"
echo "  MQTT broker:   ${MQTT_HOST}:${MQTT_PORT}"
echo "  Gateway URL:   ${GATEWAY_BROKER_URL}"
echo ""

# Check prerequisites
check_command() {
    if ! command -v "$1" &> /dev/null; then
        echo "Error: $1 is not installed. Install with: $2"
        exit 1
    fi
}

check_command "mosquitto_pub" "mise run install-tools"
check_command "mise" "curl https://mise.run | sh"

echo ""

# Step 0: Register and login user to get JWT token
echo "Step 0: Setting up authentication..."
TEST_EMAIL="mqtt-test-$(date +%s)@example.com"
TEST_PASSWORD="password123"

REGISTER_RESPONSE=$(mise exec -- grpcurl -plaintext -d "{
    \"email\": \"${TEST_EMAIL}\",
    \"password\": \"${TEST_PASSWORD}\",
    \"name\": \"MQTT Test User\"
}" "${GRPC_HOST}:${GRPC_PORT}" user.v1.UserService/RegisterUser 2>&1) || {
    echo "Failed to register user. Is the service running?"
    echo "Response: ${REGISTER_RESPONSE}"
    exit 1
}

LOGIN_RESPONSE=$(mise exec -- grpcurl -plaintext -d "{
    \"email\": \"${TEST_EMAIL}\",
    \"password\": \"${TEST_PASSWORD}\"
}" "${GRPC_HOST}:${GRPC_PORT}" user.v1.UserService/Login 2>&1) || {
    echo "Failed to login."
    echo "Response: ${LOGIN_RESPONSE}"
    exit 1
}

AUTH_TOKEN=$(echo "${LOGIN_RESPONSE}" | mise exec -- jq -r '.token')
if [ -z "${AUTH_TOKEN}" ] || [ "${AUTH_TOKEN}" = "null" ]; then
    echo "Failed to parse auth token from response:"
    echo "${LOGIN_RESPONSE}"
    exit 1
fi

echo "  Authenticated as: ${TEST_EMAIL}"
echo ""

# Step 1: Create Organization
echo "Step 1: Creating Organization..."
ORG_RESPONSE=$(mise exec -- grpcurl -plaintext \
    -H "authorization: Bearer ${AUTH_TOKEN}" \
    -d "{
    \"name\": \"Test Organization\"
}" "${GRPC_HOST}:${GRPC_PORT}" organization.v1.OrganizationService/CreateOrganization 2>&1) || {
    echo "Failed to create organization. Is the service running?"
    echo "Response: ${ORG_RESPONSE}"
    exit 1
}

ORG_ID=$(echo "${ORG_RESPONSE}" | mise exec -- jq -r '.organizationId // .organization.organizationId // empty')
if [ -z "${ORG_ID}" ]; then
    echo "Failed to parse organization ID from response:"
    echo "${ORG_RESPONSE}"
    exit 1
fi

echo "  Organization created: ${ORG_ID}"
echo ""

# Step 2: Create Gateway (will trigger CDC event and start MQTT subscriber)
echo "Step 2: Creating Gateway..."
GATEWAY_RESPONSE=$(mise exec -- grpcurl -plaintext \
    -H "authorization: Bearer ${AUTH_TOKEN}" \
    -d "{
    \"organization_id\": \"${ORG_ID}\",
    \"name\": \"Test EMQX Gateway\",
    \"type\": \"GATEWAY_TYPE_EMQX\",
    \"emqx_config\": {
        \"broker_url\": \"${GATEWAY_BROKER_URL}\",
        \"subscription_group\": \"ponix\"
    }
}" "${GRPC_HOST}:${GRPC_PORT}" gateway.v1.GatewayService/CreateGateway 2>&1) || {
    echo "Failed to create gateway. Is the service running?"
    echo "Response: ${GATEWAY_RESPONSE}"
    exit 1
}

GATEWAY_ID=$(echo "${GATEWAY_RESPONSE}" | mise exec -- jq -r '.gateway.gatewayId // .gatewayId // empty')
if [ -z "${GATEWAY_ID}" ]; then
    echo "Failed to parse gateway ID from response:"
    echo "${GATEWAY_RESPONSE}"
    exit 1
fi

echo "  Gateway created: ${GATEWAY_ID}"
echo ""

# Step 3: Create End Device Definition with Cayenne LPP conversion
echo "Step 3: Creating End Device Definition..."
DEFINITION_RESPONSE=$(mise exec -- grpcurl -plaintext \
    -H "authorization: Bearer ${AUTH_TOKEN}" \
    -d "{
    \"organization_id\": \"${ORG_ID}\",
    \"name\": \"${DEFINITION_NAME}\",
    \"json_schema\": \"${JSON_SCHEMA}\",
    \"payload_conversion\": \"${PAYLOAD_CONVERSION}\"
}" "${GRPC_HOST}:${GRPC_PORT}" end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition 2>&1) || {
    echo "Failed to create device definition."
    echo "Response: ${DEFINITION_RESPONSE}"
    exit 1
}

DEFINITION_ID=$(echo "${DEFINITION_RESPONSE}" | mise exec -- jq -r '.endDeviceDefinition.id // .id // empty')
if [ -z "${DEFINITION_ID}" ]; then
    echo "Failed to parse definition ID from response:"
    echo "${DEFINITION_RESPONSE}"
    exit 1
fi

echo "  Definition created: ${DEFINITION_ID}"
echo "  Payload conversion: ${PAYLOAD_CONVERSION}"
echo ""

# Step 4: Create End Device referencing the definition
echo "Step 4: Creating End Device..."
DEVICE_RESPONSE=$(mise exec -- grpcurl -plaintext \
    -H "authorization: Bearer ${AUTH_TOKEN}" \
    -d "{
    \"organization_id\": \"${ORG_ID}\",
    \"definition_id\": \"${DEFINITION_ID}\",
    \"name\": \"${DEVICE_NAME}\"
}" "${GRPC_HOST}:${GRPC_PORT}" end_device.v1.EndDeviceService/CreateEndDevice 2>&1) || {
    echo "Failed to create device."
    echo "Response: ${DEVICE_RESPONSE}"
    exit 1
}

DEVICE_ID=$(echo "${DEVICE_RESPONSE}" | mise exec -- jq -r '.endDevice.deviceId // .deviceId // empty')
if [ -z "${DEVICE_ID}" ]; then
    echo "Failed to parse device ID from response:"
    echo "${DEVICE_RESPONSE}"
    exit 1
fi

echo "  Device created: ${DEVICE_ID}"
echo "  Definition ID: ${DEFINITION_ID}"
echo ""

# Step 5: Wait for gateway to connect
echo "Step 5: Waiting for gateway to connect to MQTT broker..."
sleep 3
echo "  Done"
echo ""

# Step 6: Build Cayenne LPP payload
# Cayenne LPP format: [channel] [type] [data...]
# Temperature (type 0x67): 2 bytes, 0.1°C resolution, signed
# Humidity (type 0x68): 1 byte, 0.5% resolution
#
# Example: Channel 1 Temperature 27.2°C = 0x01 0x67 0x01 0x10 (272 in big-endian)
#          Channel 2 Humidity 65%       = 0x02 0x68 0x82 (130 = 65 * 2)

# Temperature: 25.5°C = 255 = 0x00FF
# Humidity: 50% = 100 = 0x64
CAYENNE_PAYLOAD_HEX="016700FF026864"

echo "Step 6: Publishing Cayenne LPP message..."
echo "  Topic: ${ORG_ID}/${DEVICE_ID}"
echo "  Payload (hex): ${CAYENNE_PAYLOAD_HEX}"
echo "  Decoded: Temperature 25.5°C, Humidity 50%"
echo ""

# Convert hex to binary and publish
echo -n "${CAYENNE_PAYLOAD_HEX}" | xxd -r -p | mosquitto_pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -s

echo "  Message published!"
echo ""

# Summary
echo "=== Test Complete ==="
echo ""
echo "Summary:"
echo "  Organization ID: ${ORG_ID}"
echo "  Gateway ID:      ${GATEWAY_ID}"
echo "  Definition ID:   ${DEFINITION_ID}"
echo "  Device ID:       ${DEVICE_ID}"
echo ""
echo "The message should have been:"
echo "  1. Received by the MQTT gateway"
echo "  2. Published to NATS as a RawEnvelope"
echo "  3. Processed by the analytics worker"
echo "  4. Stored in ClickHouse as a ProcessedEnvelope"
echo ""
echo "Check the logs with: docker logs ponix-all-in-one 2>&1 | grep ${DEVICE_ID}"
echo "Query ClickHouse with:"
echo "  docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix \\"
echo "    -q \"SELECT * FROM ponix.processed_envelopes WHERE end_device_id = '${DEVICE_ID}'\""
echo ""

# Optional: Publish a few more messages
echo "Publishing 2 more messages with different values..."

# Temperature: 30.0°C = 300 = 0x012C, Humidity: 70% = 140 = 0x8C
echo -n "0167012C02688C" | xxd -r -p | mosquitto_pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -s
echo "  Published: Temperature 30.0°C, Humidity 70%"

sleep 1

# Temperature: 18.5°C = 185 = 0x00B9, Humidity: 45% = 90 = 0x5A
echo -n "016700B902685A" | xxd -r -p | mosquitto_pub \
    -h "${MQTT_HOST}" \
    -p "${MQTT_PORT}" \
    -t "${ORG_ID}/${DEVICE_ID}" \
    -s
echo "  Published: Temperature 18.5°C, Humidity 45%"

echo ""
echo "Done! Check Grafana at http://localhost:3000 for traces."
