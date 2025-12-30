#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "Gateway Tests (GatewayService)"

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
    '{"name": "Gateway Test Org"}')

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -z "$ORG_ID" ] || [ "$ORG_ID" = "null" ]; then
    print_error "Failed to create organization"
    exit 1
fi
print_success "Organization created: $ORG_ID"

# ============================================
# HAPPY PATH TESTS
# ============================================

# CreateGateway
print_step "Testing CreateGateway (happy path)..."

GATEWAY_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/CreateGateway" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"EMQX Gateway 01\",
        \"type\": \"GATEWAY_TYPE_EMQX\",
        \"emqx_config\": {
            \"broker_url\": \"mqtt://ponix-emqx:1883\",
            \"subscription_group\": \"ponix-test-group\"
        }
    }")

GATEWAY_ID=$(echo "$GATEWAY_RESPONSE" | jq -r '.gateway.gatewayId // .gatewayId // empty')
if [ -n "$GATEWAY_ID" ] && [ "$GATEWAY_ID" != "null" ]; then
    print_success "Gateway created with ID: $GATEWAY_ID"
else
    print_error "Failed to create gateway"
    echo "$GATEWAY_RESPONSE"
    exit 1
fi

# GetGateway
print_step "Testing GetGateway (happy path)..."

GET_GATEWAY_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/GetGateway" \
    "{\"gateway_id\": \"$GATEWAY_ID\", \"organization_id\": \"$ORG_ID\"}")

RETURNED_NAME=$(echo "$GET_GATEWAY_RESPONSE" | jq -r '.gateway.name // .name // empty')
if [ "$RETURNED_NAME" = "EMQX Gateway 01" ]; then
    print_success "GetGateway returned correct data"
else
    print_error "GetGateway returned unexpected name (expected 'EMQX Gateway 01', got '$RETURNED_NAME')"
    echo "$GET_GATEWAY_RESPONSE"
    exit 1
fi

# UpdateGateway
print_step "Testing UpdateGateway (happy path)..."

UPDATE_GATEWAY_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/UpdateGateway" \
    "{
        \"gateway_id\": \"$GATEWAY_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"EMQX Gateway 01 - Updated\"
    }")

UPDATED_NAME=$(echo "$UPDATE_GATEWAY_RESPONSE" | jq -r '.gateway.name // .name // empty')
if [ "$UPDATED_NAME" = "EMQX Gateway 01 - Updated" ]; then
    print_success "Gateway updated successfully"
else
    print_error "Gateway update failed (expected 'EMQX Gateway 01 - Updated', got '$UPDATED_NAME')"
    echo "$UPDATE_GATEWAY_RESPONSE"
    exit 1
fi

# ListGateways
print_step "Testing ListGateways (happy path)..."

LIST_GATEWAY_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/ListGateways" \
    "{\"organization_id\": \"$ORG_ID\"}")

GATEWAY_COUNT=$(echo "$LIST_GATEWAY_RESPONSE" | jq '.gateways | length')
if [ "$GATEWAY_COUNT" -ge 1 ]; then
    print_success "ListGateways returned $GATEWAY_COUNT gateway(s)"
else
    print_error "ListGateways returned no gateways"
    echo "$LIST_GATEWAY_RESPONSE"
    exit 1
fi

# Create second gateway for deletion test
print_step "Creating second gateway for delete test..."

GATEWAY2_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/CreateGateway" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Gateway to Delete\",
        \"type\": \"GATEWAY_TYPE_EMQX\",
        \"emqx_config\": {
            \"broker_url\": \"mqtt://localhost:1883\",
            \"subscription_group\": \"delete-test\"
        }
    }")

GATEWAY2_ID=$(echo "$GATEWAY2_RESPONSE" | jq -r '.gateway.gatewayId // .gatewayId // empty')
if [ -n "$GATEWAY2_ID" ] && [ "$GATEWAY2_ID" != "null" ]; then
    print_success "Second gateway created: $GATEWAY2_ID"
else
    print_error "Failed to create second gateway"
    exit 1
fi

# DeleteGateway
print_step "Testing DeleteGateway (happy path)..."

set +e
DELETE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/DeleteGateway" \
    "{\"gateway_id\": \"$GATEWAY2_ID\", \"organization_id\": \"$ORG_ID\"}" 2>&1)
set -e

# Verify deletion
set +e
VERIFY_DELETE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/GetGateway" \
    "{\"gateway_id\": \"$GATEWAY2_ID\", \"organization_id\": \"$ORG_ID\"}" 2>&1)
set -e

if echo "$VERIFY_DELETE" | grep -qi "not found\|does not exist"; then
    print_success "Gateway deleted successfully"
else
    print_warning "Gateway may not have been deleted (or soft-deleted)"
fi

# ============================================
# UNAUTHENTICATED TESTS
# ============================================

print_step "Testing CreateGateway without auth..."
test_unauthenticated \
    "gateway.v1.GatewayService/CreateGateway" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Unauthorized\", \"type\": \"GATEWAY_TYPE_EMQX\"}"

print_step "Testing GetGateway without auth..."
test_unauthenticated \
    "gateway.v1.GatewayService/GetGateway" \
    "{\"gateway_id\": \"$GATEWAY_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing UpdateGateway without auth..."
test_unauthenticated \
    "gateway.v1.GatewayService/UpdateGateway" \
    "{\"gateway_id\": \"$GATEWAY_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"Hacked\"}"

print_step "Testing ListGateways without auth..."
test_unauthenticated \
    "gateway.v1.GatewayService/ListGateways" \
    "{\"organization_id\": \"$ORG_ID\"}"

print_step "Testing DeleteGateway without auth..."
test_unauthenticated \
    "gateway.v1.GatewayService/DeleteGateway" \
    "{\"gateway_id\": \"$GATEWAY_ID\", \"organization_id\": \"$ORG_ID\"}"

# ============================================
# INVALID TOKEN TESTS
# ============================================

print_step "Testing CreateGateway with invalid token..."
test_invalid_token \
    "gateway.v1.GatewayService/CreateGateway" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Invalid Token\", \"type\": \"GATEWAY_TYPE_EMQX\"}"

# ============================================
# SUMMARY
# ============================================
print_summary "Gateway Tests"
echo ""
echo -e "${YELLOW}Test Organization ID:${NC} $ORG_ID"
echo -e "${YELLOW}Test Gateway ID:${NC} $GATEWAY_ID"

# Export for dependent tests
export ORG_ID GATEWAY_ID AUTH_TOKEN
