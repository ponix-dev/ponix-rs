#!/bin/bash

# Common utilities for Ponix test scripts
# Source this file: source "$SCRIPT_DIR/lib/common.sh"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Default configuration
GRPC_HOST="${GRPC_HOST:-localhost:50051}"

# Print functions
print_step() {
    echo -e "${GREEN}>>> $1${NC}"
}

print_success() {
    echo -e "${GREEN}    [PASS] $1${NC}"
}

print_error() {
    echo -e "${RED}    [FAIL] $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}    [WARN] $1${NC}"
}

print_info() {
    echo -e "${BLUE}    $1${NC}"
}

print_result() {
    echo -e "${YELLOW}Result:${NC}"
    echo "$1" | jq '.' 2>/dev/null || echo "$1"
}

# Check if a command exists
require_command() {
    local cmd="$1"
    local install_hint="$2"
    if ! command -v "$cmd" &> /dev/null; then
        print_error "$cmd is not installed. Install with: $install_hint"
        exit 1
    fi
}

# Generate unique test email
generate_test_email() {
    echo "test-$(date +%s%N)@example.com"
}

# Register a new user and return the user ID
# Usage: USER_ID=$(register_user "$email" "$password" "$name")
register_user() {
    local email="$1"
    local password="$2"
    local name="${3:-Test User}"

    local response
    response=$(grpcurl -plaintext \
        -d "{\"email\": \"$email\", \"password\": \"$password\", \"name\": \"$name\"}" \
        "$GRPC_HOST" user.v1.UserService/RegisterUser 2>&1)

    if echo "$response" | grep -q "error\|Error"; then
        echo "REGISTER_ERROR: $response" >&2
        return 1
    fi

    echo "$response" | jq -r '.user.userId'
}

# Login and return the JWT token
# Usage: AUTH_TOKEN=$(login_user "$email" "$password")
login_user() {
    local email="$1"
    local password="$2"

    local response
    response=$(grpcurl -plaintext \
        -d "{\"email\": \"$email\", \"password\": \"$password\"}" \
        "$GRPC_HOST" user.v1.UserService/Login 2>&1)

    if echo "$response" | grep -q "error\|Error\|UNAUTHENTICATED"; then
        echo "LOGIN_ERROR: $response" >&2
        return 1
    fi

    echo "$response" | jq -r '.token'
}

# Register and login in one step
# Usage: setup_test_auth  (sets AUTH_TOKEN, TEST_USER_ID, TEST_EMAIL)
# Note: Don't use in subshell - call directly to set globals
setup_test_auth() {
    TEST_EMAIL=$(generate_test_email)
    local password="password123"
    local name="Test User"

    # Register
    local reg_response
    reg_response=$(grpcurl -plaintext \
        -d "{\"email\": \"$TEST_EMAIL\", \"password\": \"$password\", \"name\": \"$name\"}" \
        "$GRPC_HOST" user.v1.UserService/RegisterUser 2>&1)

    if echo "$reg_response" | grep -qi "error" && ! echo "$reg_response" | grep -qi "already exists"; then
        echo "REGISTER_ERROR: $reg_response" >&2
        return 1
    fi

    TEST_USER_ID=$(echo "$reg_response" | jq -r '.user.id // .user.userId // empty')

    # Login
    local login_response
    login_response=$(grpcurl -plaintext \
        -d "{\"email\": \"$TEST_EMAIL\", \"password\": \"$password\"}" \
        "$GRPC_HOST" user.v1.UserService/Login 2>&1)

    if echo "$login_response" | grep -q "UNAUTHENTICATED"; then
        echo "LOGIN_ERROR: $login_response" >&2
        return 1
    fi

    AUTH_TOKEN=$(echo "$login_response" | jq -r '.token')

    if [ -z "$AUTH_TOKEN" ] || [ "$AUTH_TOKEN" = "null" ]; then
        echo "Failed to get auth token" >&2
        return 1
    fi
}

# Legacy function for compatibility - prefer setup_test_auth
# Usage: AUTH_TOKEN=$(register_and_login)
register_and_login() {
    local email
    email=$(generate_test_email)
    local password="password123"
    local name="Test User"

    # Register
    local reg_response
    reg_response=$(grpcurl -plaintext \
        -d "{\"email\": \"$email\", \"password\": \"$password\", \"name\": \"$name\"}" \
        "$GRPC_HOST" user.v1.UserService/RegisterUser 2>&1)

    if echo "$reg_response" | grep -qi "error" && ! echo "$reg_response" | grep -qi "already exists"; then
        echo "REGISTER_ERROR: $reg_response" >&2
        return 1
    fi

    # Login
    local login_response
    login_response=$(grpcurl -plaintext \
        -d "{\"email\": \"$email\", \"password\": \"$password\"}" \
        "$GRPC_HOST" user.v1.UserService/Login 2>&1)

    if echo "$login_response" | grep -q "UNAUTHENTICATED"; then
        echo "LOGIN_ERROR: $login_response" >&2
        return 1
    fi

    echo "$login_response" | jq -r '.token'
}

# Test that an endpoint returns UNAUTHENTICATED without auth
# Usage: test_unauthenticated "service.Method" '{"field": "value"}'
test_unauthenticated() {
    local method="$1"
    local payload="$2"
    local description="${3:-$method without token}"

    set +e
    local response
    response=$(grpcurl -plaintext \
        -d "$payload" \
        "$GRPC_HOST" "$method" 2>&1)
    local exit_code=$?
    set -e

    if echo "$response" | grep -qi "unauthenticated\|missing authorization"; then
        print_success "$description correctly rejected"
        return 0
    else
        print_error "$description should return UNAUTHENTICATED"
        print_info "Got: $response"
        return 1
    fi
}

# Test that an endpoint returns UNAUTHENTICATED with invalid token
# Usage: test_invalid_token "service.Method" '{"field": "value"}'
test_invalid_token() {
    local method="$1"
    local payload="$2"
    local description="${3:-$method with invalid token}"

    set +e
    local response
    response=$(grpcurl -plaintext \
        -H "authorization: Bearer invalid.token.here" \
        -d "$payload" \
        "$GRPC_HOST" "$method" 2>&1)
    local exit_code=$?
    set -e

    if echo "$response" | grep -qi "unauthenticated\|invalid\|expired"; then
        print_success "$description correctly rejected"
        return 0
    else
        print_error "$description should return UNAUTHENTICATED"
        print_info "Got: $response"
        return 1
    fi
}

# Make authenticated gRPC call
# Usage: response=$(grpc_call "$AUTH_TOKEN" "service.Method" '{"field": "value"}')
grpc_call() {
    local token="$1"
    local method="$2"
    local payload="$3"

    grpcurl -plaintext \
        -H "authorization: Bearer $token" \
        -d "$payload" \
        "$GRPC_HOST" "$method"
}

# Make authenticated gRPC call with verbose output (for cookies)
# Usage: response=$(grpc_call_verbose "$AUTH_TOKEN" "service.Method" '{"field": "value"}'
grpc_call_verbose() {
    local token="$1"
    local method="$2"
    local payload="$3"

    grpcurl -plaintext -v \
        -H "authorization: Bearer $token" \
        -d "$payload" \
        "$GRPC_HOST" "$method" 2>&1
}

# Extract JSON from verbose grpcurl output
extract_json_response() {
    local verbose_output="$1"
    echo "$verbose_output" | sed -n '/^{/,/^}/p'
}

# Check prerequisites
check_prerequisites() {
    require_command "grpcurl" "brew install grpcurl"
    require_command "jq" "brew install jq"
}

# Initialize script (call at start of each test script)
init_test_script() {
    local script_name="$1"

    check_prerequisites

    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}$script_name${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
    echo -e "${YELLOW}gRPC Host: $GRPC_HOST${NC}"
    echo ""
}

# Print test summary footer
print_summary() {
    local script_name="$1"
    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "${GREEN}$script_name - All Tests Passed${NC}"
    echo -e "${BLUE}========================================${NC}"
}

# Track test counts (optional)
TESTS_PASSED=0
TESTS_FAILED=0

increment_passed() {
    ((TESTS_PASSED++))
}

increment_failed() {
    ((TESTS_FAILED++))
}

print_test_counts() {
    echo ""
    echo -e "${BLUE}Test Results: ${GREEN}$TESTS_PASSED passed${NC}, ${RED}$TESTS_FAILED failed${NC}"
}
