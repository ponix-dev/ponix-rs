#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "Document Updates Stream (JetStream)"

# ============================================
# PREREQUISITES
# ============================================

require_command "nats" "brew install nats-io/nats-tools/nats"

NATS_URL="${NATS_URL:-nats://localhost:4222}"

print_step "Checking NATS connectivity..."
if nats -s "$NATS_URL" account info &>/dev/null; then
    print_success "NATS is reachable at $NATS_URL"
else
    print_error "Cannot reach NATS at $NATS_URL"
    exit 1
fi

# ============================================
# STREAM EXISTENCE
# ============================================

STREAM_NAME="${PONIX_NATS_DOCUMENT_UPDATES_STREAM:-document_sync}"

print_step "Verifying stream '$STREAM_NAME' exists..."
STREAM_INFO=$(nats -s "$NATS_URL" stream info "$STREAM_NAME" --json 2>/dev/null)

if [ $? -ne 0 ] || [ -z "$STREAM_INFO" ]; then
    print_error "Stream '$STREAM_NAME' does not exist"
    exit 1
fi
print_success "Stream '$STREAM_NAME' exists"

# ============================================
# STREAM CONFIGURATION
# ============================================

print_step "Verifying stream subjects..."
SUBJECTS=$(echo "$STREAM_INFO" | jq -r '.config.subjects[]')
EXPECTED_SUBJECT="${STREAM_NAME}.*"

if echo "$SUBJECTS" | grep -qF "$EXPECTED_SUBJECT"; then
    print_success "Stream has correct subject pattern: $EXPECTED_SUBJECT"
else
    print_error "Expected subject '$EXPECTED_SUBJECT', got: $SUBJECTS"
    exit 1
fi

print_step "Verifying stream max_age (7 days = 604800s)..."
MAX_AGE_NS=$(echo "$STREAM_INFO" | jq -r '.config.max_age')
# NATS returns max_age in nanoseconds
EXPECTED_MAX_AGE_NS=604800000000000

if [ "$MAX_AGE_NS" = "$EXPECTED_MAX_AGE_NS" ]; then
    print_success "Stream max_age is 7 days (604800s)"
else
    # Convert to seconds for readability
    MAX_AGE_S=$((MAX_AGE_NS / 1000000000))
    print_error "Expected max_age 604800s, got ${MAX_AGE_S}s (${MAX_AGE_NS}ns)"
    exit 1
fi

print_step "Verifying stream description..."
DESCRIPTION=$(echo "$STREAM_INFO" | jq -r '.config.description // empty')
if [ -n "$DESCRIPTION" ]; then
    print_success "Stream has description: $DESCRIPTION"
else
    print_warning "Stream has no description (non-critical)"
fi

# ============================================
# MESSAGE PUBLISH TEST
# ============================================

print_step "Publishing test message to ${STREAM_NAME}.test-doc-123..."
nats -s "$NATS_URL" pub "${STREAM_NAME}.test-doc-123" "test-update-payload" 2>/dev/null

if [ $? -eq 0 ]; then
    print_success "Test message published"
else
    print_error "Failed to publish test message"
    exit 1
fi

print_step "Verifying message is stored in stream..."
UPDATED_INFO=$(nats -s "$NATS_URL" stream info "$STREAM_NAME" --json 2>/dev/null)
MSG_COUNT=$(echo "$UPDATED_INFO" | jq -r '.state.messages')

if [ "$MSG_COUNT" -ge 1 ]; then
    print_success "Stream contains $MSG_COUNT message(s)"
else
    print_error "Stream has no messages after publish"
    exit 1
fi

# ============================================
# SUMMARY
# ============================================
print_summary "Document Updates Stream Tests"
echo ""
echo -e "${YELLOW}Stream Name:${NC} $STREAM_NAME"
echo -e "${YELLOW}Subjects:${NC} $SUBJECTS"
echo -e "${YELLOW}Max Age:${NC} 7 days"
echo -e "${YELLOW}Messages:${NC} $MSG_COUNT"
