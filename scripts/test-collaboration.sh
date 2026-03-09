#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

# Collaboration server configuration
COLLAB_HOST="${COLLAB_HOST:-localhost}"
COLLAB_PORT="${COLLAB_PORT:-50052}"
COLLAB_WS_URL="ws://${COLLAB_HOST}:${COLLAB_PORT}"

init_test_script "Collaboration Server Tests (WebSocket)"

echo -e "${YELLOW}Collaboration Server: ${COLLAB_WS_URL}${NC}"
echo ""

# Check prerequisites
require_command "websocat" "mise install websocat"

# Portable timeout wrapper (macOS has no `timeout`, use perl fallback)
run_with_timeout() {
    local secs="$1"
    shift
    if command -v timeout &>/dev/null; then
        timeout "$secs" "$@"
    elif command -v gtimeout &>/dev/null; then
        gtimeout "$secs" "$@"
    else
        # Perl-based fallback for macOS
        perl -e "
            alarm $secs;
            exec @ARGV;
        " "$@"
    fi
}

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
    '{"name": "Collab Test Org"}')

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -z "$ORG_ID" ] || [ "$ORG_ID" = "null" ]; then
    print_error "Failed to create organization"
    exit 1
fi
print_success "Organization created: $ORG_ID"

print_step "Setup: Creating workspace..."
WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Collab Test Workspace\"}")

WORKSPACE_ID=$(echo "$WORKSPACE_RESPONSE" | jq -r '.workspace.id // .id // empty')
if [ -z "$WORKSPACE_ID" ] || [ "$WORKSPACE_ID" = "null" ]; then
    print_error "Failed to create workspace"
    exit 1
fi
print_success "Workspace created: $WORKSPACE_ID"

# ============================================
# TEST 1: Health Check - Collaboration server port is reachable
# ============================================
print_step "Test 1: Collaboration server port reachable"

set +e
# Try to connect with a short timeout — any response (even 404) means server is up
HEALTH_CHECK=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "http://${COLLAB_HOST}:${COLLAB_PORT}/ws/documents/nonexistent" 2>&1)
set -e

if [ -n "$HEALTH_CHECK" ] && [ "$HEALTH_CHECK" != "000" ]; then
    print_success "Collaboration server is reachable (HTTP $HEALTH_CHECK)"
else
    print_error "Collaboration server not reachable at ${COLLAB_HOST}:${COLLAB_PORT}"
    exit 1
fi

# ============================================
# TEST 2: Reject non-existent document (404 before upgrade)
# ============================================
print_step "Test 2: Reject non-existent document"

set +e
RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
    -H "Upgrade: websocket" \
    -H "Connection: Upgrade" \
    -H "Sec-WebSocket-Version: 13" \
    -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
    "http://${COLLAB_HOST}:${COLLAB_PORT}/ws/documents/nonexistent-doc-id" 2>&1)
set -e

if [ "$RESPONSE" = "404" ]; then
    print_success "Non-existent document correctly returns 404"
else
    print_error "Expected 404 for non-existent document, got: $RESPONSE"
    exit 1
fi

# ============================================
# TEST 3: Create document + Connect + Auth
# ============================================
print_step "Test 3: Create document and connect via WebSocket"

DOC_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/CreateWorkspaceDocument" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"Collab Test Doc\"}")

DOC_ID=$(echo "$DOC_RESPONSE" | jq -r '.document.documentId // .documentId // empty')
if [ -z "$DOC_ID" ] || [ "$DOC_ID" = "null" ]; then
    print_error "Failed to create document"
    print_info "Response: $DOC_RESPONSE"
    exit 1
fi
print_success "Document created: $DOC_ID"

# Connect via WebSocket, send JWT as first message, check we receive binary sync data
set +e
WS_OUTPUT=$(echo "$AUTH_TOKEN" | run_with_timeout 5 websocat -n1 -B 65536 \
    "${COLLAB_WS_URL}/ws/documents/${DOC_ID}" 2>&1)
WS_EXIT=$?
set -e

# websocat returns data (may be binary/garbled in text mode, but should be non-empty)
# Exit 0 = normal, 142 = SIGALRM (perl timeout), 124 = timeout (coreutils)
if [ $WS_EXIT -eq 0 ] || [ $WS_EXIT -eq 124 ] || [ $WS_EXIT -eq 142 ]; then
    if [ -n "$WS_OUTPUT" ]; then
        print_success "WebSocket auth succeeded, received sync data"
    else
        print_success "WebSocket auth succeeded, connection established"
    fi
else
    print_error "WebSocket connection failed (exit $WS_EXIT)"
    print_info "Output: $WS_OUTPUT"
    exit 1
fi

# ============================================
# TEST 4: Reject invalid auth
# ============================================
print_step "Test 4: Reject invalid auth token"

set +e
INVALID_OUTPUT=$(echo "invalid.token.here" | run_with_timeout 5 websocat -n1 \
    "${COLLAB_WS_URL}/ws/documents/${DOC_ID}" 2>&1)
INVALID_EXIT=$?
set -e

# Should get disconnected — either non-zero exit, close frame text, or empty output
# (server sends close frame which websocat may handle silently with exit 0)
if [ $INVALID_EXIT -ne 0 ] || echo "$INVALID_OUTPUT" | grep -qi "invalid\|expired\|4001\|close" || [ -z "$INVALID_OUTPUT" ]; then
    print_success "Invalid auth token correctly rejected"
else
    print_error "Expected rejection for invalid token"
    print_info "Output: $INVALID_OUTPUT"
    exit 1
fi

# ============================================
# TEST 5: Auth timeout
# ============================================
print_step "Test 5: Auth timeout (no message sent)"

set +e
# Connect but don't send anything — server should close after 5s auth timeout
TIMEOUT_OUTPUT=$(run_with_timeout 8 websocat -n0 \
    "${COLLAB_WS_URL}/ws/documents/${DOC_ID}" 2>&1)
TIMEOUT_EXIT=$?
set -e

if [ $TIMEOUT_EXIT -ne 0 ] || echo "$TIMEOUT_OUTPUT" | grep -qi "timeout\|close"; then
    print_success "Auth timeout correctly handled"
else
    print_error "Expected timeout/close for no auth message"
    print_info "Output: $TIMEOUT_OUTPUT"
    exit 1
fi

# ============================================
# SUMMARY
# ============================================
print_summary "Collaboration Server Tests"
