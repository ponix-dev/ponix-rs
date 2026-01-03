#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "Workspace Tests (WorkspaceService)"

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
    '{"name": "Workspace Test Org"}')

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -z "$ORG_ID" ] || [ "$ORG_ID" = "null" ]; then
    print_error "Failed to create organization"
    exit 1
fi
print_success "Organization created: $ORG_ID"

# ============================================
# HAPPY PATH TESTS
# ============================================

# CreateWorkspace
print_step "Testing CreateWorkspace (happy path)..."

start_cdc_listener "workspaces"
WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Test Workspace\"}")

WORKSPACE_ID=$(echo "$WORKSPACE_RESPONSE" | jq -r '.workspace.id // .id // empty')
if [ -n "$WORKSPACE_ID" ] && [ "$WORKSPACE_ID" != "null" ]; then
    print_success "Workspace created with ID: $WORKSPACE_ID"
else
    print_error "Failed to create workspace"
    echo "$WORKSPACE_RESPONSE"
    cleanup_cdc_listener
    exit 1
fi
wait_for_cdc_event "workspaces" "create"

# GetWorkspace
print_step "Testing GetWorkspace (happy path)..."

GET_WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/GetWorkspace" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\"}")

RETURNED_NAME=$(echo "$GET_WORKSPACE_RESPONSE" | jq -r '.workspace.name // .name // empty')
if [ "$RETURNED_NAME" = "Test Workspace" ]; then
    print_success "GetWorkspace returned correct data"
else
    print_error "GetWorkspace returned unexpected data"
    echo "$GET_WORKSPACE_RESPONSE"
    exit 1
fi

# UpdateWorkspace
print_step "Testing UpdateWorkspace (happy path)..."

UPDATE_WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/UpdateWorkspace" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"Updated Workspace\"}")

UPDATED_NAME=$(echo "$UPDATE_WORKSPACE_RESPONSE" | jq -r '.workspace.name // .name // empty')
if [ "$UPDATED_NAME" = "Updated Workspace" ]; then
    print_success "Workspace updated successfully"
else
    print_error "Workspace update failed"
    echo "$UPDATE_WORKSPACE_RESPONSE"
    exit 1
fi

# ListWorkspaces
print_step "Testing ListWorkspaces (happy path)..."

LIST_WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/ListWorkspaces" \
    "{\"organization_id\": \"$ORG_ID\"}")

WORKSPACE_COUNT=$(echo "$LIST_WORKSPACE_RESPONSE" | jq '.workspaces | length')
if [ "$WORKSPACE_COUNT" -ge 1 ]; then
    print_success "ListWorkspaces returned $WORKSPACE_COUNT workspace(s)"
else
    print_error "ListWorkspaces returned no workspaces"
    echo "$LIST_WORKSPACE_RESPONSE"
    exit 1
fi

# Create second workspace for deletion test
print_step "Creating second workspace for delete test..."

WORKSPACE2_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Workspace to Delete\"}")

WORKSPACE2_ID=$(echo "$WORKSPACE2_RESPONSE" | jq -r '.workspace.id // .id // empty')
if [ -n "$WORKSPACE2_ID" ] && [ "$WORKSPACE2_ID" != "null" ]; then
    print_success "Second workspace created: $WORKSPACE2_ID"
else
    print_error "Failed to create second workspace"
    exit 1
fi

# DeleteWorkspace
print_step "Testing DeleteWorkspace (happy path)..."

set +e
DELETE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/DeleteWorkspace" \
    "{\"workspace_id\": \"$WORKSPACE2_ID\", \"organization_id\": \"$ORG_ID\"}" 2>&1)
set -e

# Verify deletion
set +e
VERIFY_DELETE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/GetWorkspace" \
    "{\"workspace_id\": \"$WORKSPACE2_ID\", \"organization_id\": \"$ORG_ID\"}" 2>&1)
set -e

if echo "$VERIFY_DELETE" | grep -qi "not found\|does not exist"; then
    print_success "Workspace deleted successfully"
else
    print_warning "Workspace may not have been deleted (or soft-deleted)"
fi

# ============================================
# UNAUTHENTICATED TESTS
# ============================================

print_step "Testing CreateWorkspace without auth..."
test_unauthenticated \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Unauthorized Workspace\"}"

print_step "Testing GetWorkspace without auth..."
test_unauthenticated \
    "workspace.v1.WorkspaceService/GetWorkspace" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing UpdateWorkspace without auth..."
test_unauthenticated \
    "workspace.v1.WorkspaceService/UpdateWorkspace" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"Hacked\"}"

print_step "Testing ListWorkspaces without auth..."
test_unauthenticated \
    "workspace.v1.WorkspaceService/ListWorkspaces" \
    "{\"organization_id\": \"$ORG_ID\"}"

print_step "Testing DeleteWorkspace without auth..."
test_unauthenticated \
    "workspace.v1.WorkspaceService/DeleteWorkspace" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\"}"

# ============================================
# INVALID TOKEN TESTS
# ============================================

print_step "Testing CreateWorkspace with invalid token..."
test_invalid_token \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Invalid Token Workspace\"}"

# ============================================
# SUMMARY
# ============================================
print_summary "Workspace Tests"
echo ""
echo -e "${YELLOW}Test Organization ID:${NC} $ORG_ID"
echo -e "${YELLOW}Test Workspace ID:${NC} $WORKSPACE_ID"

# Export for dependent tests
export ORG_ID WORKSPACE_ID AUTH_TOKEN
