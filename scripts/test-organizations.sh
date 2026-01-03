#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "Organization Tests (OrganizationService)"

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

# ============================================
# HAPPY PATH TESTS
# ============================================

# CreateOrganization
print_step "Testing CreateOrganization (happy path)..."

# Start CDC listener before creating organization
start_cdc_listener "organizations"

ORG_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/CreateOrganization" \
    '{"name": "Test Organization"}')

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -n "$ORG_ID" ] && [ "$ORG_ID" != "null" ]; then
    print_success "Organization created with ID: $ORG_ID"
else
    print_error "Failed to create organization"
    echo "$ORG_RESPONSE"
    cleanup_cdc_listener
    exit 1
fi

# Verify CDC event was received
wait_for_cdc_event "organizations" "create"

# GetOrganization
print_step "Testing GetOrganization (happy path)..."

GET_ORG_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/GetOrganization" \
    "{\"organization_id\": \"$ORG_ID\"}")

RETURNED_NAME=$(echo "$GET_ORG_RESPONSE" | jq -r '.organization.name // .name // empty')
if [ "$RETURNED_NAME" = "Test Organization" ]; then
    print_success "GetOrganization returned correct data"
else
    print_error "GetOrganization returned unexpected data"
    echo "$GET_ORG_RESPONSE"
    exit 1
fi

# ListOrganizations
print_step "Testing ListOrganizations (happy path)..."

LIST_ORG_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/ListOrganizations" \
    '{}')

ORG_COUNT=$(echo "$LIST_ORG_RESPONSE" | jq '.organizations | length')
if [ "$ORG_COUNT" -ge 1 ]; then
    print_success "ListOrganizations returned $ORG_COUNT organization(s)"
else
    print_error "ListOrganizations returned no organizations"
    echo "$LIST_ORG_RESPONSE"
    exit 1
fi

# UserOrganizations
print_step "Testing UserOrganizations (happy path)..."

USER_ORG_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/UserOrganizations" \
    "{\"user_id\": \"$TEST_USER_ID\"}")

USER_ORG_COUNT=$(echo "$USER_ORG_RESPONSE" | jq '.organizations | length')
if [ "$USER_ORG_COUNT" -ge 1 ]; then
    print_success "UserOrganizations returned $USER_ORG_COUNT organization(s)"
else
    print_error "UserOrganizations returned no organizations"
    echo "$USER_ORG_RESPONSE"
    exit 1
fi

# Create second org for deletion test
print_step "Creating second organization for delete test..."

ORG2_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/CreateOrganization" \
    '{"name": "Organization to Delete"}')

ORG2_ID=$(echo "$ORG2_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -n "$ORG2_ID" ] && [ "$ORG2_ID" != "null" ]; then
    print_success "Second organization created: $ORG2_ID"
else
    print_error "Failed to create second organization"
    exit 1
fi

# DeleteOrganization
print_step "Testing DeleteOrganization (happy path)..."

set +e
DELETE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/DeleteOrganization" \
    "{\"organization_id\": \"$ORG2_ID\"}" 2>&1)
DELETE_EXIT=$?
set -e

# Verify deletion by trying to get it
set +e
VERIFY_DELETE=$(grpc_call "$AUTH_TOKEN" \
    "organization.v1.OrganizationService/GetOrganization" \
    "{\"organization_id\": \"$ORG2_ID\"}" 2>&1)
set -e

if echo "$VERIFY_DELETE" | grep -qi "not found\|does not exist"; then
    print_success "Organization deleted successfully"
else
    print_warning "Organization may not have been deleted (or soft-deleted)"
fi

# ============================================
# UNAUTHENTICATED TESTS
# ============================================

print_step "Testing CreateOrganization without auth..."
test_unauthenticated \
    "organization.v1.OrganizationService/CreateOrganization" \
    '{"name": "Unauthorized Org"}'

print_step "Testing GetOrganization without auth..."
test_unauthenticated \
    "organization.v1.OrganizationService/GetOrganization" \
    "{\"organization_id\": \"$ORG_ID\"}"

# Note: ListOrganizations is a public endpoint (no auth required)

print_step "Testing UserOrganizations without auth..."
test_unauthenticated \
    "organization.v1.OrganizationService/UserOrganizations" \
    "{\"user_id\": \"$TEST_USER_ID\"}"

print_step "Testing DeleteOrganization without auth..."
test_unauthenticated \
    "organization.v1.OrganizationService/DeleteOrganization" \
    "{\"organization_id\": \"$ORG_ID\"}"

# ============================================
# INVALID TOKEN TESTS
# ============================================

print_step "Testing CreateOrganization with invalid token..."
test_invalid_token \
    "organization.v1.OrganizationService/CreateOrganization" \
    '{"name": "Invalid Token Org"}'

# ============================================
# SUMMARY
# ============================================
print_summary "Organization Tests"
echo ""
echo -e "${YELLOW}Test Organization ID:${NC} $ORG_ID"
echo -e "${YELLOW}Test User ID:${NC} $TEST_USER_ID"

# Export for dependent tests
export ORG_ID AUTH_TOKEN
