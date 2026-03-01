#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "Data Stream Tests (DataStreamService + DataStreamDefinitionService)"

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
    '{"name": "Data Stream Test Org"}')

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -z "$ORG_ID" ] || [ "$ORG_ID" = "null" ]; then
    print_error "Failed to create organization"
    exit 1
fi
print_success "Organization created: $ORG_ID"

print_step "Setup: Creating workspace..."
WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Data Stream Test Workspace\"}")

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
        \"name\": \"Data Stream Test Gateway\",
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
# DATA STREAM DEFINITION HAPPY PATH TESTS
# ============================================

# CreateDataStreamDefinition
print_step "Testing CreateDataStreamDefinition (happy path)..."

DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamDefinitionService/CreateDataStreamDefinition" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Cayenne LPP Temperature Sensor\",
        \"contracts\": [{
            \"match_expression\": \"true\",
            \"transform_expression\": \"cayenne_lpp_decode(input)\",
            \"json_schema\": \"{\\\"type\\\": \\\"object\\\"}\"
        }]
    }")

DEFINITION_ID=$(echo "$DEFINITION_RESPONSE" | jq -r '.dataStreamDefinition.id // .id // empty')
if [ -n "$DEFINITION_ID" ] && [ "$DEFINITION_ID" != "null" ]; then
    print_success "Definition created with ID: $DEFINITION_ID"
else
    print_error "Failed to create definition"
    echo "$DEFINITION_RESPONSE"
    exit 1
fi

# GetDataStreamDefinition
print_step "Testing GetDataStreamDefinition (happy path)..."

GET_DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamDefinitionService/GetDataStreamDefinition" \
    "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}")

RETURNED_DEF_NAME=$(echo "$GET_DEFINITION_RESPONSE" | jq -r '.dataStreamDefinition.name // .name // empty')
if [ "$RETURNED_DEF_NAME" = "Cayenne LPP Temperature Sensor" ]; then
    print_success "GetDataStreamDefinition returned correct data"
else
    print_error "GetDataStreamDefinition returned unexpected data"
    echo "$GET_DEFINITION_RESPONSE"
    exit 1
fi

# UpdateDataStreamDefinition
print_step "Testing UpdateDataStreamDefinition (happy path)..."

UPDATE_DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamDefinitionService/UpdateDataStreamDefinition" \
    "{
        \"id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Cayenne LPP Sensor - Updated\",
        \"contracts\": [{
            \"match_expression\": \"true\",
            \"transform_expression\": \"cayenne_lpp_decode(input)\",
            \"json_schema\": \"{\\\"type\\\": \\\"object\\\", \\\"description\\\": \\\"Updated\\\"}\"
        }]
    }")

UPDATED_DEF_NAME=$(echo "$UPDATE_DEFINITION_RESPONSE" | jq -r '.dataStreamDefinition.name // .name // empty')
if [ "$UPDATED_DEF_NAME" = "Cayenne LPP Sensor - Updated" ]; then
    print_success "Definition updated successfully"
else
    print_error "Definition update failed"
    echo "$UPDATE_DEFINITION_RESPONSE"
    exit 1
fi

# ListDataStreamDefinitions
print_step "Testing ListDataStreamDefinitions (happy path)..."

LIST_DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamDefinitionService/ListDataStreamDefinitions" \
    "{\"organization_id\": \"$ORG_ID\"}")

DEFINITION_COUNT=$(echo "$LIST_DEFINITION_RESPONSE" | jq '.dataStreamDefinitions | length')
if [ "$DEFINITION_COUNT" -ge 1 ]; then
    print_success "ListDataStreamDefinitions returned $DEFINITION_COUNT definition(s)"
else
    print_error "ListDataStreamDefinitions returned no definitions"
    echo "$LIST_DEFINITION_RESPONSE"
    exit 1
fi

# ============================================
# DATA STREAM HAPPY PATH TESTS
# ============================================

# CreateDataStream
print_step "Testing CreateDataStream (happy path)..."

start_cdc_listener "data_streams"
DATA_STREAM_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamService/CreateDataStream" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"gateway_id\": \"$GATEWAY_ID\",
        \"name\": \"Temperature Sensor Alpha\"
    }")

DATA_STREAM_ID=$(echo "$DATA_STREAM_RESPONSE" | jq -r '.dataStream.dataStreamId // .dataStreamId // empty')
if [ -n "$DATA_STREAM_ID" ] && [ "$DATA_STREAM_ID" != "null" ]; then
    print_success "Data stream created with ID: $DATA_STREAM_ID"
else
    print_error "Failed to create data stream"
    echo "$DATA_STREAM_RESPONSE"
    cleanup_cdc_listener
    exit 1
fi
wait_for_cdc_event "data_streams" "create"

# GetDataStream
print_step "Testing GetDataStream (happy path)..."

GET_DATA_STREAM_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamService/GetDataStream" \
    "{\"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}")

RETURNED_DATA_STREAM_NAME=$(echo "$GET_DATA_STREAM_RESPONSE" | jq -r '.dataStream.name // .name // empty')
if [ "$RETURNED_DATA_STREAM_NAME" = "Temperature Sensor Alpha" ]; then
    print_success "GetDataStream returned correct data"
else
    print_error "GetDataStream returned unexpected data"
    echo "$GET_DATA_STREAM_RESPONSE"
    exit 1
fi

# GetWorkspaceDataStreams
print_step "Testing GetWorkspaceDataStreams (happy path)..."

LIST_DATA_STREAM_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamService/GetWorkspaceDataStreams" \
    "{\"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}")

DATA_STREAM_COUNT=$(echo "$LIST_DATA_STREAM_RESPONSE" | jq '.dataStreams | length')
if [ "$DATA_STREAM_COUNT" -ge 1 ]; then
    print_success "GetWorkspaceDataStreams returned $DATA_STREAM_COUNT data stream(s)"
else
    print_error "GetWorkspaceDataStreams returned no data streams"
    echo "$LIST_DATA_STREAM_RESPONSE"
    exit 1
fi

# GetGatewayDataStreams
print_step "Testing GetGatewayDataStreams (happy path)..."

LIST_GATEWAY_DATA_STREAMS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamService/GetGatewayDataStreams" \
    "{\"organization_id\": \"$ORG_ID\", \"gateway_id\": \"$GATEWAY_ID\"}")

GATEWAY_DATA_STREAM_COUNT=$(echo "$LIST_GATEWAY_DATA_STREAMS_RESPONSE" | jq '.dataStreams | length')
if [ "$GATEWAY_DATA_STREAM_COUNT" -ge 1 ]; then
    print_success "GetGatewayDataStreams returned $GATEWAY_DATA_STREAM_COUNT data stream(s)"
else
    print_error "GetGatewayDataStreams returned no data streams"
    echo "$LIST_GATEWAY_DATA_STREAMS_RESPONSE"
    exit 1
fi

# Verify gateway_id in returned data stream
RETURNED_GATEWAY_ID=$(echo "$LIST_GATEWAY_DATA_STREAMS_RESPONSE" | jq -r '.dataStreams[0].gatewayId // empty')
if [ "$RETURNED_GATEWAY_ID" = "$GATEWAY_ID" ]; then
    print_success "Data stream has correct gateway_id"
else
    print_error "Data stream gateway_id mismatch: expected $GATEWAY_ID, got $RETURNED_GATEWAY_ID"
    exit 1
fi

# ============================================
# DEFINITION DELETE TEST (after data stream cleanup consideration)
# ============================================

# Create a definition specifically for deletion test
print_step "Creating definition for delete test..."

DEF2_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamDefinitionService/CreateDataStreamDefinition" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Definition to Delete\",
        \"contracts\": [{
            \"match_expression\": \"true\",
            \"transform_expression\": \"input\",
            \"json_schema\": \"{}\"
        }]
    }")

DEFINITION2_ID=$(echo "$DEF2_RESPONSE" | jq -r '.dataStreamDefinition.id // .id // empty')
if [ -n "$DEFINITION2_ID" ] && [ "$DEFINITION2_ID" != "null" ]; then
    print_success "Second definition created: $DEFINITION2_ID"
else
    print_error "Failed to create second definition"
    exit 1
fi

# DeleteDataStreamDefinition
print_step "Testing DeleteDataStreamDefinition (happy path)..."

set +e
DELETE_DEF_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamDefinitionService/DeleteDataStreamDefinition" \
    "{\"id\": \"$DEFINITION2_ID\", \"organization_id\": \"$ORG_ID\"}" 2>&1)
set -e

# Verify deletion
set +e
VERIFY_DEF_DELETE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamDefinitionService/GetDataStreamDefinition" \
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

print_step "Testing CreateDataStreamDefinition without auth..."
test_unauthenticated \
    "data_stream.v1.DataStreamDefinitionService/CreateDataStreamDefinition" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Unauthorized\"}"

print_step "Testing GetDataStreamDefinition without auth..."
test_unauthenticated \
    "data_stream.v1.DataStreamDefinitionService/GetDataStreamDefinition" \
    "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing UpdateDataStreamDefinition without auth..."
test_unauthenticated \
    "data_stream.v1.DataStreamDefinitionService/UpdateDataStreamDefinition" \
    "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"Hacked\"}"

print_step "Testing ListDataStreamDefinitions without auth..."
test_unauthenticated \
    "data_stream.v1.DataStreamDefinitionService/ListDataStreamDefinitions" \
    "{\"organization_id\": \"$ORG_ID\"}"

print_step "Testing DeleteDataStreamDefinition without auth..."
test_unauthenticated \
    "data_stream.v1.DataStreamDefinitionService/DeleteDataStreamDefinition" \
    "{\"id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}"

# ============================================
# UNAUTHENTICATED TESTS - DATA STREAMS
# ============================================

print_step "Testing CreateDataStream without auth..."
test_unauthenticated \
    "data_stream.v1.DataStreamService/CreateDataStream" \
    "{\"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\", \"definition_id\": \"$DEFINITION_ID\", \"gateway_id\": \"$GATEWAY_ID\", \"name\": \"Unauthorized\"}"

print_step "Testing GetDataStream without auth..."
test_unauthenticated \
    "data_stream.v1.DataStreamService/GetDataStream" \
    "{\"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}"

print_step "Testing GetWorkspaceDataStreams without auth..."
test_unauthenticated \
    "data_stream.v1.DataStreamService/GetWorkspaceDataStreams" \
    "{\"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}"

# ============================================
# INVALID TOKEN TESTS
# ============================================

print_step "Testing CreateDataStreamDefinition with invalid token..."
test_invalid_token \
    "data_stream.v1.DataStreamDefinitionService/CreateDataStreamDefinition" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Invalid Token\"}"

print_step "Testing CreateDataStream with invalid token..."
test_invalid_token \
    "data_stream.v1.DataStreamService/CreateDataStream" \
    "{\"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\", \"definition_id\": \"$DEFINITION_ID\", \"gateway_id\": \"$GATEWAY_ID\", \"name\": \"Invalid Token\"}"

# ============================================
# SUMMARY
# ============================================
print_summary "Data Stream Tests"
echo ""
echo -e "${YELLOW}Test Organization ID:${NC} $ORG_ID"
echo -e "${YELLOW}Test Workspace ID:${NC} $WORKSPACE_ID"
echo -e "${YELLOW}Test Gateway ID:${NC} $GATEWAY_ID"
echo -e "${YELLOW}Test Definition ID:${NC} $DEFINITION_ID"
echo -e "${YELLOW}Test Data Stream ID:${NC} $DATA_STREAM_ID"

# Export for dependent tests
export ORG_ID WORKSPACE_ID GATEWAY_ID DEFINITION_ID DATA_STREAM_ID AUTH_TOKEN
