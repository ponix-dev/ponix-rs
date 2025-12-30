#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Configuration
GRPC_HOST="${GRPC_HOST:-localhost:50051}"
TEST_EMAIL="${TEST_EMAIL:-test-$(date +%s)@example.com}"
TEST_PASSWORD="${TEST_PASSWORD:-password123}"
TEST_NAME="${TEST_NAME:-Test User}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Ponix gRPC API Test Script${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo -e "${YELLOW}Note: This script requires grpcurl. Install with: brew install grpcurl${NC}"
echo -e "${YELLOW}gRPC reflection is enabled, so service discovery works automatically.${NC}"
echo ""

# Function to print step headers
print_step() {
    echo -e "${GREEN}▶ $1${NC}"
}

# Function to print results
print_result() {
    echo -e "${YELLOW}Result:${NC}"
    echo "$1" | jq '.'
    echo ""
}

# Step 1: Register User
print_step "Step 1: Registering user..."
echo -e "${YELLOW}Email: $TEST_EMAIL${NC}"

REGISTER_RESPONSE=$(grpcurl -plaintext \
    -d "{
        \"email\": \"$TEST_EMAIL\",
        \"password\": \"$TEST_PASSWORD\",
        \"name\": \"$TEST_NAME\"
    }" \
    "$GRPC_HOST" user.v1.UserService/RegisterUser)

print_result "$REGISTER_RESPONSE"

USER_ID=$(echo "$REGISTER_RESPONSE" | jq -r '.user.userId')
echo -e "${GREEN}✓ User registered with ID: $USER_ID${NC}"
echo ""

# Step 2: Login User
print_step "Step 2: Logging in user..."
LOGIN_RESPONSE=$(grpcurl -plaintext \
    -d "{
        \"email\": \"$TEST_EMAIL\",
        \"password\": \"$TEST_PASSWORD\"
    }" \
    "$GRPC_HOST" user.v1.UserService/Login)

print_result "$LOGIN_RESPONSE"

# Extract JWT token
AUTH_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token')
echo -e "${GREEN}✓ Login successful, JWT token obtained${NC}"
echo ""

# Step 3: Test unauthenticated organization creation (should fail)
print_step "Step 3: Testing unauthenticated organization creation (should fail)..."
echo -e "${YELLOW}Attempting to create organization without JWT token...${NC}"

set +e  # Temporarily allow errors
UNAUTH_RESPONSE=$(grpcurl -plaintext \
    -d '{
        "name": "Unauthorized Org"
    }' \
    "$GRPC_HOST" organization.v1.OrganizationService/CreateOrganization 2>&1)
UNAUTH_EXIT_CODE=$?
set -e  # Re-enable exit on error

if echo "$UNAUTH_RESPONSE" | grep -qi "unauthenticated\|missing authorization"; then
    echo -e "${GREEN}✓ Correctly rejected unauthenticated request${NC}"
    echo -e "${YELLOW}Response:${NC} $UNAUTH_RESPONSE"
else
    echo -e "${RED}✗ Unexpected response (exit code: $UNAUTH_EXIT_CODE):${NC}"
    echo "$UNAUTH_RESPONSE"
    exit 1
fi
echo ""

# Step 4: Test invalid token (should fail)
print_step "Step 4: Testing invalid JWT token (should fail)..."
echo -e "${YELLOW}Attempting to create organization with invalid token...${NC}"

set +e  # Temporarily allow errors
INVALID_TOKEN_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer invalid.token.here" \
    -d '{
        "name": "Invalid Token Org"
    }' \
    "$GRPC_HOST" organization.v1.OrganizationService/CreateOrganization 2>&1)
INVALID_EXIT_CODE=$?
set -e  # Re-enable exit on error

if echo "$INVALID_TOKEN_RESPONSE" | grep -qi "unauthenticated\|invalid\|expired"; then
    echo -e "${GREEN}✓ Correctly rejected invalid token${NC}"
    echo -e "${YELLOW}Response:${NC} $INVALID_TOKEN_RESPONSE"
else
    echo -e "${RED}✗ Unexpected response (exit code: $INVALID_EXIT_CODE):${NC}"
    echo "$INVALID_TOKEN_RESPONSE"
    exit 1
fi
echo ""

# Step 5: Create Organization (requires authentication)
print_step "Step 5: Creating organization (with valid authentication)..."
ORG_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d '{
        "name": "Test Organization"
    }' \
    "$GRPC_HOST" organization.v1.OrganizationService/CreateOrganization)

print_result "$ORG_RESPONSE"

# Extract organization ID
ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId')
echo -e "${GREEN}✓ Organization created with ID: $ORG_ID${NC}"
echo ""

# Step 5b: Create Workspace
print_step "Step 5b: Creating workspace..."
WORKSPACE_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Default Workspace\"
    }" \
    "$GRPC_HOST" workspace.v1.WorkspaceService/CreateWorkspace)

print_result "$WORKSPACE_RESPONSE"

# Extract workspace ID
WORKSPACE_ID=$(echo "$WORKSPACE_RESPONSE" | jq -r '.workspace.id')
echo -e "${GREEN}✓ Workspace created with ID: $WORKSPACE_ID${NC}"
echo ""

# Step 6: Verify user-organization link and RBAC role assignment
print_step "Step 6: Verifying user-organization link and RBAC role..."
echo -e "${YELLOW}To verify the link in the database, run:${NC}"
echo -e "${BLUE}docker exec -it ponix-postgres psql -U ponix -d ponix -c \"SELECT * FROM user_organizations WHERE user_id = '$USER_ID';\"${NC}"
echo ""
echo -e "${YELLOW}To verify RBAC role assignment (user should be admin of the org):${NC}"
echo -e "${BLUE}docker exec -it ponix-postgres psql -U ponix -d ponix -c \"SELECT * FROM casbin_rule WHERE v0 = '$USER_ID' AND v2 = '$ORG_ID';\"${NC}"
echo ""

# Step 7: Create End Device Definition with Cayenne LPP converter
print_step "Step 7: Creating end device definition with Cayenne LPP payload converter..."
DEFINITION_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Cayenne LPP Temperature Sensor\",
        \"json_schema\": \"{\\\"type\\\": \\\"object\\\"}\",
        \"payload_conversion\": \"cayenne_lpp.decode(payload)\"
    }" \
    "$GRPC_HOST" end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition)

print_result "$DEFINITION_RESPONSE"

# Extract definition ID
DEFINITION_ID=$(echo "$DEFINITION_RESPONSE" | jq -r '.endDeviceDefinition.id')
echo -e "${GREEN}✓ End device definition created with ID: $DEFINITION_ID${NC}"
echo -e "${YELLOW}  Payload Converter: cayenne_lpp.decode(payload)${NC}"
echo ""

# Step 7b: Create End Device referencing the definition
print_step "Step 7b: Creating end device using the definition..."
DEVICE_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"name\": \"Temperature Sensor Alpha\"
    }" \
    "$GRPC_HOST" end_device.v1.EndDeviceService/CreateEndDevice)

print_result "$DEVICE_RESPONSE"

# Extract device ID
DEVICE_ID=$(echo "$DEVICE_RESPONSE" | jq -r '.endDevice.deviceId')
echo -e "${GREEN}✓ End device created with ID: $DEVICE_ID${NC}"
echo -e "${YELLOW}  Definition ID: $DEFINITION_ID${NC}"
echo ""

# Step 8: Create Gateway
print_step "Step 8: Creating EMQX gateway..."
GATEWAY_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"EMQX Gateway 01\",
        \"type\": \"GATEWAY_TYPE_EMQX\",
        \"emqx_config\": {
            \"broker_url\": \"mqtt://ponix-emqx:1883\",
            \"subscription_group\": \"ponix-test-group\"
        }
    }" \
    "$GRPC_HOST" gateway.v1.GatewayService/CreateGateway)

print_result "$GATEWAY_RESPONSE"

# Extract gateway ID
GATEWAY_ID=$(echo "$GATEWAY_RESPONSE" | jq -r '.gateway.gatewayId')
echo -e "${GREEN}✓ Gateway created with ID: $GATEWAY_ID${NC}"
echo ""

# Step 9: Verify - List all resources
print_step "Step 9: Verifying created resources..."

echo -e "${BLUE}Listing end device definitions for organization...${NC}"
DEFINITIONS_LIST=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{\"organization_id\": \"$ORG_ID\"}" \
    "$GRPC_HOST" end_device.v1.EndDeviceDefinitionService/ListEndDeviceDefinitions)
print_result "$DEFINITIONS_LIST"

echo -e "${BLUE}Listing end devices for organization...${NC}"
DEVICES_LIST=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{\"organization_id\": \"$ORG_ID\"}" \
    "$GRPC_HOST" end_device.v1.EndDeviceService/ListEndDevices)
print_result "$DEVICES_LIST"

echo -e "${BLUE}Listing gateways for organization...${NC}"
GATEWAYS_LIST=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{\"organization_id\": \"$ORG_ID\"}" \
    "$GRPC_HOST" gateway.v1.GatewayService/ListGateways)
print_result "$GATEWAYS_LIST"

# Step 10: Get individual resources
print_step "Step 10: Retrieving individual resources..."

echo -e "${BLUE}Getting organization details...${NC}"
ORG_DETAILS=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{\"organization_id\": \"$ORG_ID\"}" \
    "$GRPC_HOST" organization.v1.OrganizationService/GetOrganization)
print_result "$ORG_DETAILS"

echo -e "${BLUE}Getting end device definition details...${NC}"
DEFINITION_DETAILS=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}" \
    "$GRPC_HOST" end_device.v1.EndDeviceDefinitionService/GetEndDeviceDefinition)
print_result "$DEFINITION_DETAILS"

echo -e "${BLUE}Getting end device details...${NC}"
DEVICE_DETAILS=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{\"device_id\": \"$DEVICE_ID\", \"organization_id\": \"$ORG_ID\"}" \
    "$GRPC_HOST" end_device.v1.EndDeviceService/GetEndDevice)
print_result "$DEVICE_DETAILS"

echo -e "${BLUE}Getting gateway details...${NC}"
GATEWAY_DETAILS=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{\"gateway_id\": \"$GATEWAY_ID\", \"organization_id\": \"$ORG_ID\"}" \
    "$GRPC_HOST" gateway.v1.GatewayService/GetGateway)
print_result "$GATEWAY_DETAILS"

# Step 11: Update operations
print_step "Step 11: Testing update operations..."

echo -e "${BLUE}Updating end device definition...${NC}"
UPDATE_DEFINITION=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Cayenne LPP Temperature Sensor - Updated\",
        \"json_schema\": \"{\\\"type\\\": \\\"object\\\", \\\"description\\\": \\\"Updated schema\\\"}\",
        \"payload_conversion\": \"cayenne_lpp.decode(payload)\"
    }" \
    "$GRPC_HOST" end_device.v1.EndDeviceDefinitionService/UpdateEndDeviceDefinition)
print_result "$UPDATE_DEFINITION"

echo -e "${GREEN}✓ Definition updated successfully${NC}"
echo ""

echo -e "${BLUE}Updating gateway name...${NC}"
UPDATE_GATEWAY=$(grpcurl -plaintext \
    -H "authorization: Bearer $AUTH_TOKEN" \
    -d "{
        \"gateway_id\": \"$GATEWAY_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"EMQX Gateway 01 - Updated\"
    }" \
    "$GRPC_HOST" gateway.v1.GatewayService/UpdateGateway)
print_result "$UPDATE_GATEWAY"

echo -e "${GREEN}✓ Gateway updated successfully${NC}"
echo ""

# Summary
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test Summary${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}✓ User ID:${NC} $USER_ID"
echo -e "${GREEN}✓ Organization ID:${NC} $ORG_ID"
echo -e "${GREEN}✓ Workspace ID:${NC} $WORKSPACE_ID"
echo -e "${GREEN}✓ End Device Definition ID:${NC} $DEFINITION_ID"
echo -e "${GREEN}✓ End Device ID:${NC} $DEVICE_ID"
echo -e "${GREEN}✓ Gateway ID:${NC} $GATEWAY_ID"
echo ""
echo -e "${YELLOW}JWT Token (for further testing):${NC}"
echo "$AUTH_TOKEN"
echo ""
echo -e "${YELLOW}Verify user-organization link:${NC}"
echo "docker exec -it ponix-postgres psql -U ponix -d ponix -c \"SELECT * FROM user_organizations;\""
echo ""
echo -e "${YELLOW}Verify RBAC role assignment (admin role for org creator):${NC}"
echo "docker exec -it ponix-postgres psql -U ponix -d ponix -c \"SELECT * FROM casbin_rule WHERE ptype = 'g';\""
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Check NATS for CDC events:"
echo "   nats sub 'gateway.>'"
echo ""
echo "2. Send a raw envelope for the device:"
echo "   (This will use the Cayenne LPP converter to decode the payload)"
echo ""
echo "3. Query ClickHouse for processed envelopes:"
echo "   docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix"
echo "   SELECT * FROM ponix.processed_envelopes WHERE device_id = '$DEVICE_ID';"
echo ""
echo -e "${GREEN}All tests completed successfully!${NC}"
