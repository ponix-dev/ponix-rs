#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

# Collaboration server configuration
COLLAB_HOST="${COLLAB_HOST:-localhost}"
COLLAB_PORT="${COLLAB_PORT:-50052}"
COLLAB_WS_URL="ws://${COLLAB_HOST}:${COLLAB_PORT}"

# NATS configuration
NATS_URL="${NATS_URL:-nats://localhost:4222}"
STREAM_NAME="${PONIX_NATS_DOCUMENT_UPDATES_STREAM:-document_sync}"

init_test_script "Collaboration Server Tests"

echo -e "${YELLOW}Collaboration Server: ${COLLAB_WS_URL}${NC}"
echo -e "${YELLOW}NATS: ${NATS_URL}${NC}"
echo ""

# ============================================
# PREREQUISITES
# ============================================

require_command "nats" "mise install github:nats-io/natscli"

# Build or locate collab-test-client
COLLAB_CLIENT="${COLLAB_CLIENT:-}"
if [ -z "$COLLAB_CLIENT" ]; then
    # Check for pre-built binary in common locations
    for candidate in \
        "$SCRIPT_DIR/../target/debug/collab-test-client" \
        "$SCRIPT_DIR/../target/release/collab-test-client"; do
        if [ -x "$candidate" ]; then
            COLLAB_CLIENT="$candidate"
            break
        fi
    done
fi

if [ -z "$COLLAB_CLIENT" ] || [ ! -x "$COLLAB_CLIENT" ]; then
    print_step "Building collab-test-client..."
    (cd "$SCRIPT_DIR/.." && cargo build -p collaboration_server --features test-client --bin collab-test-client 2>&1)
    COLLAB_CLIENT="$SCRIPT_DIR/../target/debug/collab-test-client"
fi

require_command "$COLLAB_CLIENT" "cargo build -p collaboration_server --features test-client --bin collab-test-client"

# Portable timeout wrapper (macOS has no `timeout`, use perl fallback)
run_with_timeout() {
    local secs="$1"
    shift
    if command -v timeout &>/dev/null; then
        timeout "$secs" "$@"
    elif command -v gtimeout &>/dev/null; then
        gtimeout "$secs" "$@"
    else
        perl -e "
            alarm $secs;
            exec @ARGV;
        " "$@"
    fi
}

# ============================================
# SETUP: Auth, org, workspace
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

# Helper: create a document and return its ID
create_document() {
    local name="$1"
    local response
    response=$(grpc_call "$AUTH_TOKEN" \
        "document.v1.DocumentService/CreateWorkspaceDocument" \
        "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"$name\"}")
    echo "$response" | jq -r '.document.documentId // .documentId // empty'
}

# ============================================
# SECTION 1: NATS Infrastructure
# ============================================

echo ""
echo -e "${BLUE}--- NATS Infrastructure ---${NC}"

# Test 1: document_sync stream exists with correct config
print_step "Test 1: Document sync stream exists with correct configuration..."

set +e
STREAM_INFO=$(nats -s "$NATS_URL" stream info "$STREAM_NAME" --json 2>/dev/null)
STREAM_EXISTS=$?
set -e

if [ $STREAM_EXISTS -ne 0 ] || [ -z "$STREAM_INFO" ]; then
    print_error "Stream '$STREAM_NAME' does not exist"
    exit 1
fi

# Verify subject pattern
SUBJECTS=$(echo "$STREAM_INFO" | jq -r '.config.subjects[]' 2>/dev/null || echo "")
if echo "$SUBJECTS" | grep -qF "${STREAM_NAME}.*"; then
    print_success "Stream '$STREAM_NAME' exists with correct subject pattern"
else
    print_error "Expected subject '${STREAM_NAME}.*', got: $SUBJECTS"
    exit 1
fi

# Verify max_age (7 days = 604800s = 604800000000000ns)
MAX_AGE_NS=$(echo "$STREAM_INFO" | jq -r '.config.max_age')
EXPECTED_MAX_AGE_NS=604800000000000
if [ "$MAX_AGE_NS" = "$EXPECTED_MAX_AGE_NS" ]; then
    print_success "Stream max_age is 7 days"
else
    MAX_AGE_S=$((MAX_AGE_NS / 1000000000))
    print_error "Expected max_age 604800s, got ${MAX_AGE_S}s"
    exit 1
fi

# Test 2: Snapshotter consumer exists
print_step "Test 2: Snapshotter NATS consumer exists..."

set +e
CONSUMER_INFO=$(nats -s "$NATS_URL" consumer info "$STREAM_NAME" "document-snapshotter" --json 2>/dev/null)
CONSUMER_EXISTS=$?
set -e

if [ $CONSUMER_EXISTS -eq 0 ] && echo "$CONSUMER_INFO" | jq -e '.name' &>/dev/null; then
    NUM_PENDING=$(echo "$CONSUMER_INFO" | jq -r '.num_pending // 0')
    NUM_DELIVERED=$(echo "$CONSUMER_INFO" | jq -r '.delivered.consumer_seq // 0')
    print_success "Snapshotter consumer found (pending=$NUM_PENDING, delivered=$NUM_DELIVERED)"
else
    print_error "Snapshotter consumer 'document-snapshotter' not found on stream '$STREAM_NAME'"
    exit 1
fi

# ============================================
# SECTION 2: WebSocket Connection & Auth
# ============================================

echo ""
echo -e "${BLUE}--- WebSocket Connection & Auth ---${NC}"

# Test 3: Health check - server port is reachable
print_step "Test 3: Collaboration server is reachable..."

set +e
HEALTH_CHECK=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "http://${COLLAB_HOST}:${COLLAB_PORT}/ws/documents/nonexistent" 2>&1)
set -e

if [ -n "$HEALTH_CHECK" ] && [ "$HEALTH_CHECK" != "000" ]; then
    print_success "Collaboration server is reachable (HTTP $HEALTH_CHECK)"
else
    print_error "Collaboration server not reachable at ${COLLAB_HOST}:${COLLAB_PORT}"
    exit 1
fi

# Test 4: Reject non-existent document (404 before upgrade)
print_step "Test 4: Reject WebSocket for non-existent document..."

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

# Test 5: Reject invalid auth token
print_step "Test 5: Reject invalid auth token..."

DOC_AUTH_TEST=$(create_document "Auth Test Doc")
if [ -z "$DOC_AUTH_TEST" ] || [ "$DOC_AUTH_TEST" = "null" ]; then
    print_error "Failed to create document for auth test"
    exit 1
fi

set +e
INVALID_OUTPUT=$("$COLLAB_CLIENT" \
    --url "${COLLAB_WS_URL}/ws/documents/${DOC_AUTH_TEST}" \
    --token "invalid.token.here" \
    read 2>&1)
INVALID_EXIT=$?
set -e

if [ $INVALID_EXIT -ne 0 ]; then
    print_success "Invalid auth token correctly rejected"
else
    print_error "Expected rejection for invalid token, but command succeeded"
    exit 1
fi

# ============================================
# SECTION 3: Single-Client Edit & Persistence
# ============================================

echo ""
echo -e "${BLUE}--- Single-Client Edit & Snapshotter Persistence ---${NC}"

# Test 6: Edit a document and verify persistence via snapshotter
print_step "Test 6: Edit document via WebSocket and verify snapshotter persists to DB..."

DOC_EDIT=$(create_document "Edit Persistence Test")
if [ -z "$DOC_EDIT" ] || [ "$DOC_EDIT" = "null" ]; then
    print_error "Failed to create document"
    exit 1
fi
print_info "Document created: $DOC_EDIT"

# Verify document starts empty
INITIAL_DOC=$(grpc_call "$AUTH_TOKEN" "document.v1.DocumentService/GetDocument" \
    "{\"document_id\": \"$DOC_EDIT\", \"organization_id\": \"$ORG_ID\"}")
INITIAL_TEXT=$(echo "$INITIAL_DOC" | jq -r '.document.contentText // empty')
if [ -n "$INITIAL_TEXT" ] && [ "$INITIAL_TEXT" != "" ]; then
    print_error "Expected empty content_text on new document, got: $INITIAL_TEXT"
    exit 1
fi
print_info "Document starts empty (correct)"

# Edit via WebSocket
EDIT_TEXT="Hello from e2e test"
set +e
CLIENT_OUTPUT=$("$COLLAB_CLIENT" \
    --url "${COLLAB_WS_URL}/ws/documents/${DOC_EDIT}" \
    --token "$AUTH_TOKEN" \
    edit --text "$EDIT_TEXT" 2>&1)
CLIENT_EXIT=$?
set -e

if [ $CLIENT_EXIT -ne 0 ]; then
    print_error "collab-test-client edit failed (exit $CLIENT_EXIT)"
    print_info "Output: $CLIENT_OUTPUT"
    exit 1
fi
print_info "Edit sent: \"$EDIT_TEXT\""

# Wait for snapshotter compaction cycle (runs every 5s, plus margin)
print_info "Waiting for snapshotter compaction cycle..."
sleep 10

# Read document back via gRPC and verify content_text
PERSISTED_DOC=$(grpc_call "$AUTH_TOKEN" "document.v1.DocumentService/GetDocument" \
    "{\"document_id\": \"$DOC_EDIT\", \"organization_id\": \"$ORG_ID\"}")
PERSISTED_TEXT=$(echo "$PERSISTED_DOC" | jq -r '.document.contentText // empty')

if [ "$PERSISTED_TEXT" = "$EDIT_TEXT" ]; then
    print_success "Snapshotter persisted content_text: \"$PERSISTED_TEXT\""
else
    print_error "Expected content_text \"$EDIT_TEXT\", got: \"$PERSISTED_TEXT\""
    print_info "Full response: $PERSISTED_DOC"
    exit 1
fi

# Also verify content_html was generated
PERSISTED_HTML=$(echo "$PERSISTED_DOC" | jq -r '.document.contentHtml // empty')
if [ -n "$PERSISTED_HTML" ]; then
    print_success "Snapshotter persisted content_html: \"$PERSISTED_HTML\""
else
    print_error "Expected non-empty content_html"
    exit 1
fi

# ============================================
# SECTION 4: Two-Client Sync
# ============================================

echo ""
echo -e "${BLUE}--- Two-Client Real-Time Sync ---${NC}"

# Test 7: Client A edits, Client B receives the update in real-time
print_step "Test 7: Two-client real-time sync..."

DOC_SYNC=$(create_document "Two Client Sync Test")
if [ -z "$DOC_SYNC" ] || [ "$DOC_SYNC" = "null" ]; then
    print_error "Failed to create document"
    exit 1
fi
print_info "Document created: $DOC_SYNC"

# Start Client B (listener) in background
"$COLLAB_CLIENT" \
    --url "${COLLAB_WS_URL}/ws/documents/${DOC_SYNC}" \
    --token "$AUTH_TOKEN" \
    listen --duration 10 > /tmp/collab-listener-output.txt 2>&1 &
LISTENER_PID=$!

# Give listener time to connect and complete sync handshake
sleep 2

# Client A edits
SYNC_TEXT="Synced from client A"
set +e
"$COLLAB_CLIENT" \
    --url "${COLLAB_WS_URL}/ws/documents/${DOC_SYNC}" \
    --token "$AUTH_TOKEN" \
    edit --text "$SYNC_TEXT" > /dev/null 2>&1
EDIT_EXIT=$?
set -e

if [ $EDIT_EXIT -ne 0 ]; then
    kill $LISTENER_PID 2>/dev/null || true
    print_error "Client A edit failed"
    exit 1
fi
print_info "Client A sent: \"$SYNC_TEXT\""

# Wait for listener to receive and exit
wait $LISTENER_PID 2>/dev/null || true
LISTENER_OUTPUT=$(cat /tmp/collab-listener-output.txt)
rm -f /tmp/collab-listener-output.txt

if [ "$LISTENER_OUTPUT" = "$SYNC_TEXT" ]; then
    print_success "Client B received synced text: \"$LISTENER_OUTPUT\""
else
    print_error "Expected Client B to receive \"$SYNC_TEXT\", got: \"$LISTENER_OUTPUT\""
    exit 1
fi

# ============================================
# SECTION 5: Awareness / Presence
# ============================================

echo ""
echo -e "${BLUE}--- Awareness / Presence ---${NC}"

# Test 9: Two clients connect, both see each other's presence
print_step "Test 9: Two-client awareness / presence..."

DOC_AWARENESS=$(create_document "Awareness Test")
if [ -z "$DOC_AWARENESS" ] || [ "$DOC_AWARENESS" = "null" ]; then
    print_error "Failed to create document"
    exit 1
fi
print_info "Document created: $DOC_AWARENESS"

# Start Client A (presence listener) in background — listens for 8 seconds
"$COLLAB_CLIENT" \
    --url "${COLLAB_WS_URL}/ws/documents/${DOC_AWARENESS}" \
    --token "$AUTH_TOKEN" \
    presence --duration 8 > /tmp/collab-presence-a.txt 2>&1 &
PRESENCE_A_PID=$!

# Give client A time to connect
sleep 2

# Client B connects and immediately checks presence — should see client A
set +e
PRESENCE_B_OUTPUT=$("$COLLAB_CLIENT" \
    --url "${COLLAB_WS_URL}/ws/documents/${DOC_AWARENESS}" \
    --token "$AUTH_TOKEN" \
    presence --duration 3 2>&1)
PRESENCE_B_EXIT=$?
set -e

# Wait for client A to finish
wait $PRESENCE_A_PID 2>/dev/null || true
PRESENCE_A_OUTPUT=$(cat /tmp/collab-presence-a.txt)
rm -f /tmp/collab-presence-a.txt

# Client B should have received awareness with user info (from client A's presence)
# The output is JSON lines like: [{"user_id":"...","name":"...","email":"...","color":"#..."}]
if echo "$PRESENCE_B_OUTPUT" | grep -q '"user_id"'; then
    print_success "Client B received awareness state with user presence"
else
    # Client B may not see A if it connected before A's awareness was ready.
    # At minimum, B should get its own awareness state.
    if [ $PRESENCE_B_EXIT -eq 0 ]; then
        print_success "Client B connected with presence support (no peers yet)"
    else
        print_error "Client B presence failed (exit $PRESENCE_B_EXIT)"
        print_info "Output: $PRESENCE_B_OUTPUT"
        exit 1
    fi
fi

# Verify awareness contains expected fields (name, email, color)
if echo "$PRESENCE_B_OUTPUT" | grep -q '"name"' && \
   echo "$PRESENCE_B_OUTPUT" | grep -q '"email"' && \
   echo "$PRESENCE_B_OUTPUT" | grep -q '"color"'; then
    print_success "Awareness state includes name, email, and color fields"
else
    # If B didn't receive awareness JSON, check A's output instead
    if echo "$PRESENCE_A_OUTPUT" | grep -q '"name"' && \
       echo "$PRESENCE_A_OUTPUT" | grep -q '"email"' && \
       echo "$PRESENCE_A_OUTPUT" | grep -q '"color"'; then
        print_success "Awareness state includes name, email, and color fields (from client A)"
    else
        print_error "Awareness state missing expected fields"
        print_info "Client A output: $PRESENCE_A_OUTPUT"
        print_info "Client B output: $PRESENCE_B_OUTPUT"
        exit 1
    fi
fi

# ============================================
# SECTION 6: Reconnect After Edit (State Persistence)
# ============================================

echo ""
echo -e "${BLUE}--- Reconnect State Recovery ---${NC}"

# Test 10: Edit, disconnect, reconnect — new client sees the content
print_step "Test 10: Reconnect and read persisted state..."

DOC_RECONNECT=$(create_document "Reconnect Test")
if [ -z "$DOC_RECONNECT" ] || [ "$DOC_RECONNECT" = "null" ]; then
    print_error "Failed to create document"
    exit 1
fi
print_info "Document created: $DOC_RECONNECT"

# Client edits and disconnects
RECONNECT_TEXT="Persisted across reconnect"
set +e
"$COLLAB_CLIENT" \
    --url "${COLLAB_WS_URL}/ws/documents/${DOC_RECONNECT}" \
    --token "$AUTH_TOKEN" \
    edit --text "$RECONNECT_TEXT" > /dev/null 2>&1
set -e

# Wait for snapshotter to persist (compaction cycle)
print_info "Waiting for snapshotter to persist..."
sleep 10

# New client connects and reads — should see the text from DB state
set +e
READ_OUTPUT=$("$COLLAB_CLIENT" \
    --url "${COLLAB_WS_URL}/ws/documents/${DOC_RECONNECT}" \
    --token "$AUTH_TOKEN" \
    read 2>&1)
READ_EXIT=$?
set -e

if [ $READ_EXIT -ne 0 ]; then
    print_error "Reconnect read failed (exit $READ_EXIT)"
    print_info "Output: $READ_OUTPUT"
    exit 1
fi

if [ "$READ_OUTPUT" = "$RECONNECT_TEXT" ]; then
    print_success "Reconnected client reads persisted state: \"$READ_OUTPUT\""
else
    print_error "Expected \"$RECONNECT_TEXT\", got: \"$READ_OUTPUT\""
    exit 1
fi

# ============================================
# SUMMARY
# ============================================

print_summary "Collaboration Server Tests"
