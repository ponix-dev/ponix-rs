#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "Device Tests (EndDeviceService + EndDeviceDefinitionService)"

# ============================================
# SETUP
# ============================================
print_step "Setup: Authenticating..."
setup_test_auth

if [ -z "$AUTH_TOKEN" ] || [ "$AUTH_TOKEN" = "null" ]; then
    print_error "Failed to authenticate"
    exit 1
fi
print_success "Authenticated as $TEST_EMAIL"

print_step "Setup: Creating organization..."
ORG_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/CreateOrganization" \
    '{"name": "Device Test Org"}')

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -z "$ORG_ID" ] || [ "$ORG_ID" = "null" ]; then
    print_error "Failed to create organization"
    exit 1
fi
print_success "Organization created: $ORG_ID"

print_step "Setup: Creating workspace..."
WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Device Test Workspace\"}")

WORKSPACE_ID=$(echo "$WORKSPACE_RESPONSE" | jq -r '.workspace.id // .id // empty')
if [ -z "$WORKSPACE_ID" ] || [ "$WORKSPACE_ID" = "null" ]; then
    print_error "Failed to create workspace"
    exit 1
fi
print_success "Workspace created: $WORKSPACE_ID"

print_step "Setup: Creating gateway..."
GATEWAY_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/CreateGateway" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Device Test Gateway\",
        \"type\": \"GATEWAY_TYPE_EMQX\",
        \"emqx_config\": {
            \"broker_url\": \"mqtt://localhost:1883\"
        }
    }")

GATEWAY_ID=$(echo "$GATEWAY_RESPONSE" | jq -r '.gateway.gatewayId // .gatewayId // empty')
if [ -z "$GATEWAY_ID" ] || [ "$GATEWAY_ID" = "null" ]; then
    print_error "Failed to create gateway"
    exit 1
fi
print_success "Gateway created: $GATEWAY_ID"

# ============================================
# END DEVICE DEFINITION HAPPY PATH TESTS
# ============================================

# CreateEndDeviceDefinition
print_step "Testing CreateEndDeviceDefinition (happy path)..."

DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Cayenne LPP Temperature Sensor\",
        \"json_schema\": \"{\\\"type\\\": \\\"object\\\"}\",
        \"payload_conversion\": \"cayenne_lpp.decode(payload)\"
    }")

DEFINITION_ID=$(echo "$DEFINITION_RESPONSE" | jq -r '.endDeviceDefinition.id // .id // empty')
if [ -n "$DEFINITION_ID" ] && [ "$DEFINITION_ID" != "null" ]; then
    print_success "Definition created with ID: $DEFINITION_ID"
else
    print_error "Failed to create definition"
    echo "$DEFINITION_RESPONSE"
    exit 1
fi

# GetEndDeviceDefinition
print_step "Testing GetEndDeviceDefinition (happy path)..."

GET_DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/GetEndDeviceDefinition" \
    "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}")

RETURNED_DEF_NAME=$(echo "$GET_DEFINITION_RESPONSE" | jq -r '.endDeviceDefinition.name // .name // empty')
if [ "$RETURNED_DEF_NAME" = "Cayenne LPP Temperature Sensor" ]; then
    print_success "GetEndDeviceDefinition returned correct data"
else
    print_error "GetEndDeviceDefinition returned unexpected data"
    echo "$GET_DEFINITION_RESPONSE"
    exit 1
fi

# UpdateEndDeviceDefinition
print_step "Testing UpdateEndDeviceDefinition (happy path)..."

UPDATE_DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/UpdateEndDeviceDefinition" \
    "{
        \"id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Cayenne LPP Sensor - Updated\",
        \"json_schema\": \"{\\\"type\\\": \\\"object\\\", \\\"description\\\": \\\"Updated\\\"}\",
        \"payload_conversion\": \"cayenne_lpp.decode(payload)\"
    }")

UPDATED_DEF_NAME=$(echo "$UPDATE_DEFINITION_RESPONSE" | jq -r '.endDeviceDefinition.name // .name // empty')
if [ "$UPDATED_DEF_NAME" = "Cayenne LPP Sensor - Updated" ]; then
    print_success "Definition updated successfully"
else
    print_error "Definition update failed"
    echo "$UPDATE_DEFINITION_RESPONSE"
    exit 1
fi

# ListEndDeviceDefinitions
print_step "Testing ListEndDeviceDefinitions (happy path)..."

LIST_DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/ListEndDeviceDefinitions" \
    "{\"organization_id\": \"$ORG_ID\"}")

DEFINITION_COUNT=$(echo "$LIST_DEFINITION_RESPONSE" | jq '.endDeviceDefinitions | length')
if [ "$DEFINITION_COUNT" -ge 1 ]; then
    print_success "ListEndDeviceDefinitions returned $DEFINITION_COUNT definition(s)"
else
    print_error "ListEndDeviceDefinitions returned no definitions"
    echo "$LIST_DEFINITION_RESPONSE"
    exit 1
fi

# ============================================
# END DEVICE HAPPY PATH TESTS
# ============================================

# CreateEndDevice
print_step "Testing CreateEndDevice (happy path)..."

start_cdc_listener "devices"
DEVICE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceService/CreateEndDevice" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"gateway_id\": \"$GATEWAY_ID\",
        \"name\": \"Temperature Sensor Alpha\"
    }")

DEVICE_ID=$(echo "$DEVICE_RESPONSE" | jq -r '.endDevice.deviceId // .deviceId // empty')
if [ -n "$DEVICE_ID" ] && [ "$DEVICE_ID" != "null" ]; then
    print_success "Device created with ID: $DEVICE_ID"
else
    print_error "Failed to create device"
    echo "$DEVICE_RESPONSE"
    cleanup_cdc_listener
    exit 1
fi
wait_for_cdc_event "devices" "create"

# GetEndDevice
print_step "Testing GetEndDevice (happy path)..."

GET_DEVICE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceService/GetEndDevice" \
    "{\"device_id\": \"$DEVICE_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}")

RETURNED_DEVICE_NAME=$(echo "$GET_DEVICE_RESPONSE" | jq -r '.endDevice.name // .name // empty')
if [ "$RETURNED_DEVICE_NAME" = "Temperature Sensor Alpha" ]; then
    print_success "GetEndDevice returned correct data"
else
    print_error "GetEndDevice returned unexpected data"
    echo "$GET_DEVICE_RESPONSE"
    exit 1
fi

# GetWorkspaceEndDevices
print_step "Testing GetWorkspaceEndDevices (happy path)..."

LIST_DEVICE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceService/GetWorkspaceEndDevices" \
    "{\"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}")

DEVICE_COUNT=$(echo "$LIST_DEVICE_RESPONSE" | jq '.endDevices | length')
if [ "$DEVICE_COUNT" -ge 1 ]; then
    print_success "GetWorkspaceEndDevices returned $DEVICE_COUNT device(s)"
else
    print_error "GetWorkspaceEndDevices returned no devices"
    echo "$LIST_DEVICE_RESPONSE"
    exit 1
fi

# GetGatewayEndDevices
print_step "Testing GetGatewayEndDevices (happy path)..."

LIST_GATEWAY_DEVICES_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceService/GetGatewayEndDevices" \
    "{\"organization_id\": \"$ORG_ID\", \"gateway_id\": \"$GATEWAY_ID\"}")

GATEWAY_DEVICE_COUNT=$(echo "$LIST_GATEWAY_DEVICES_RESPONSE" | jq '.endDevices | length')
if [ "$GATEWAY_DEVICE_COUNT" -ge 1 ]; then
    print_success "GetGatewayEndDevices returned $GATEWAY_DEVICE_COUNT device(s)"
else
    print_error "GetGatewayEndDevices returned no devices"
    echo "$LIST_GATEWAY_DEVICES_RESPONSE"
    exit 1
fi

# Verify gateway_id in returned device
RETURNED_GATEWAY_ID=$(echo "$LIST_GATEWAY_DEVICES_RESPONSE" | jq -r '.endDevices[0].gatewayId // empty')
if [ "$RETURNED_GATEWAY_ID" = "$GATEWAY_ID" ]; then
    print_success "Device has correct gateway_id"
else
    print_error "Device gateway_id mismatch: expected $GATEWAY_ID, got $RETURNED_GATEWAY_ID"
    exit 1
fi

# ============================================
# DEFINITION DELETE TEST (after device cleanup consideration)
# ============================================

# Create a definition specifically for deletion test
print_step "Creating definition for delete test..."

DEF2_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Definition to Delete\",
        \"json_schema\": \"{}\",
        \"payload_conversion\": \"input\"
    }")

DEFINITION2_ID=$(echo "$DEF2_RESPONSE" | jq -r '.endDeviceDefinition.id // .id // empty')
if [ -n "$DEFINITION2_ID" ] && [ "$DEFINITION2_ID" != "null" ]; then
    print_success "Second definition created: $DEFINITION2_ID"
else
    print_error "Failed to create second definition"
    exit 1
fi

# DeleteEndDeviceDefinition
print_step "Testing DeleteEndDeviceDefinition (happy path)..."

set +e
DELETE_DEF_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/DeleteEndDeviceDefinition" \
    "{\"id\": \"$DEFINITION2_ID\", \"organization_id\": \"$ORG_ID\"}" 2>&1)
set -e

# Verify deletion
set +e
VERIFY_DEF_DELETE=$(grpc_call "$AUTH_TOKEN" \
    "end_device.v1.EndDeviceDefinitionService/GetEndDeviceDefinition" \
    "{\"id\": \"$DEFINITION2_ID\", \"organization_id\": \"$ORG_ID\"}" 2>&1)
set -e

if echo "$VERIFY_DEF_DELETE" | grep -qi "not found\|does not exist"; then
    print_success "Definition deleted successfully"
else
    print_warning "Definition may not have been deleted (or soft-deleted)"
fi

# ============================================
# UNAUTHENTICATED TESTS - DEFINITIONS
# ============================================

print_step "Testing CreateEndDeviceDefinition without auth..."
test_unauthenticated \
    "end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Unauthorized\"}"

print_step "Testing GetEndDeviceDefinition without auth..."
test_unauthenticated \
    "end_device.v1.EndDeviceDefinitionService/GetEndDeviceDefinition" \
    "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing UpdateEndDeviceDefinition without auth..."
test_unauthenticated \
    "end_device.v1.EndDeviceDefinitionService/UpdateEndDeviceDefinition" \
    "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"Hacked\"}"

print_step "Testing ListEndDeviceDefinitions without auth..."
test_unauthenticated \
    "end_device.v1.EndDeviceDefinitionService/ListEndDeviceDefinitions" \
    "{\"organization_id\": \"$ORG_ID\"}"

print_step "Testing DeleteEndDeviceDefinition without auth..."
test_unauthenticated \
    "end_device.v1.EndDeviceDefinitionService/DeleteEndDeviceDefinition" \
    "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}"

# ============================================
# UNAUTHENTICATED TESTS - DEVICES
# ============================================

print_step "Testing CreateEndDevice without auth..."
test_unauthenticated \
    "end_device.v1.EndDeviceService/CreateEndDevice" \
    "{\"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\", \"definition_id\": \"$DEFINITION_ID\", \"gateway_id\": \"$GATEWAY_ID\", \"name\": \"Unauthorized\"}"

print_step "Testing GetEndDevice without auth..."
test_unauthenticated \
    "end_device.v1.EndDeviceService/GetEndDevice" \
    "{\"device_id\": \"$DEVICE_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}"

print_step "Testing GetWorkspaceEndDevices without auth..."
test_unauthenticated \
    "end_device.v1.EndDeviceService/GetWorkspaceEndDevices" \
    "{\"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}"

# ============================================
# INVALID TOKEN TESTS
# ============================================

print_step "Testing CreateEndDeviceDefinition with invalid token..."
test_invalid_token \
    "end_device.v1.EndDeviceDefinitionService/CreateEndDeviceDefinition" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Invalid Token\"}"

print_step "Testing CreateEndDevice with invalid token..."
test_invalid_token \
    "end_device.v1.EndDeviceService/CreateEndDevice" \
    "{\"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\", \"definition_id\": \"$DEFINITION_ID\", \"gateway_id\": \"$GATEWAY_ID\", \"name\": \"Invalid Token\"}"

# ============================================
# SUMMARY
# ============================================
print_summary "Device Tests"
echo ""
echo -e "${YELLOW}Test Organization ID:${NC} $ORG_ID"
echo -e "${YELLOW}Test Workspace ID:${NC} $WORKSPACE_ID"
echo -e "${YELLOW}Test Gateway ID:${NC} $GATEWAY_ID"
echo -e "${YELLOW}Test Definition ID:${NC} $DEFINITION_ID"
echo -e "${YELLOW}Test Device ID:${NC} $DEVICE_ID"

# Export for dependent tests
export ORG_ID WORKSPACE_ID GATEWAY_ID DEFINITION_ID DEVICE_ID AUTH_TOKEN
