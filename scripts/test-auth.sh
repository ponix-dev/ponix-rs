#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "Authentication Tests (UserService)"

# Test data
TEST_EMAIL=$(generate_test_email)
TEST_PASSWORD="securepassword123"
TEST_NAME="Auth Test User"

# ============================================
# HAPPY PATH TESTS
# ============================================

# Step 1: Register user
print_step "Testing RegisterUser (happy path)..."

REGISTER_RESPONSE=$(grpcurl -plaintext \
    -d "{\"email\": \"$TEST_EMAIL\", \"password\": \"$TEST_PASSWORD\", \"name\": \"$TEST_NAME\"}" \
    "$GRPC_HOST" user.v1.UserService/RegisterUser)

USER_ID=$(echo "$REGISTER_RESPONSE" | jq -r '.user.id // .user.userId // empty')
if [ -n "$USER_ID" ] && [ "$USER_ID" != "null" ]; then
    print_success "User registered with ID: $USER_ID"
else
    print_error "Failed to register user"
    echo "$REGISTER_RESPONSE"
    exit 1
fi

# Step 2: Login
print_step "Testing Login (happy path)..."

LOGIN_OUTPUT=$(grpcurl -plaintext -v \
    -d "{\"email\": \"$TEST_EMAIL\", \"password\": \"$TEST_PASSWORD\"}" \
    "$GRPC_HOST" user.v1.UserService/Login 2>&1)

LOGIN_RESPONSE=$(extract_json_response "$LOGIN_OUTPUT")
ACCESS_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token')

if [ -n "$ACCESS_TOKEN" ] && [ "$ACCESS_TOKEN" != "null" ]; then
    print_success "Login successful, JWT token received"
    print_info "Token: ${ACCESS_TOKEN:0:50}..."
else
    print_error "Failed to get access token"
    echo "$LOGIN_RESPONSE"
    exit 1
fi

# Extract refresh token from cookie
REFRESH_TOKEN=""
SET_COOKIE_LINE=$(echo "$LOGIN_OUTPUT" | grep -i "set-cookie" || true)
if [ -n "$SET_COOKIE_LINE" ]; then
    REFRESH_TOKEN=$(echo "$SET_COOKIE_LINE" | sed -n 's/.*refresh_token=\([^;]*\).*/\1/p' | head -1)
fi

if [ -n "$REFRESH_TOKEN" ]; then
    print_success "Refresh token cookie received"
    print_info "Token: ${REFRESH_TOKEN:0:50}..."
else
    print_warning "No refresh token cookie found"
fi

# Step 3: GetUser with valid token
print_step "Testing GetUser (happy path)..."

GET_USER_RESPONSE=$(grpcurl -plaintext \
    -H "authorization: Bearer $ACCESS_TOKEN" \
    -d "{\"user_id\": \"$USER_ID\"}" \
    "$GRPC_HOST" user.v1.UserService/GetUser)

RETURNED_USER_ID=$(echo "$GET_USER_RESPONSE" | jq -r '.user.id // .user.userId // .id // .userId // empty')
if [ "$RETURNED_USER_ID" = "$USER_ID" ]; then
    print_success "GetUser returned correct user"
else
    print_error "GetUser returned unexpected user"
    echo "$GET_USER_RESPONSE"
    exit 1
fi

# Step 4: Refresh token
print_step "Testing Refresh (happy path)..."

if [ -n "$REFRESH_TOKEN" ]; then
    REFRESH_OUTPUT=$(grpcurl -plaintext -v \
        -H "cookie: refresh_token=$REFRESH_TOKEN" \
        -d "{}" \
        "$GRPC_HOST" user.v1.UserService/Refresh 2>&1)

    if echo "$REFRESH_OUTPUT" | grep -qi "accesstoken\|access_token"; then
        REFRESH_RESPONSE=$(extract_json_response "$REFRESH_OUTPUT")
        NEW_ACCESS_TOKEN=$(echo "$REFRESH_RESPONSE" | jq -r '.accessToken // .access_token')
        print_success "Token refresh successful"
        print_info "New token: ${NEW_ACCESS_TOKEN:0:50}..."

        # Check for token rotation
        NEW_SET_COOKIE=$(echo "$REFRESH_OUTPUT" | grep -i "set-cookie" || true)
        if [ -n "$NEW_SET_COOKIE" ]; then
            NEW_REFRESH_TOKEN=$(echo "$NEW_SET_COOKIE" | sed -n 's/.*refresh_token=\([^;]*\).*/\1/p' | head -1)
            if [ -n "$NEW_REFRESH_TOKEN" ] && [ "$NEW_REFRESH_TOKEN" != "$REFRESH_TOKEN" ]; then
                print_success "Refresh token rotated"
                REFRESH_TOKEN="$NEW_REFRESH_TOKEN"
            fi
        fi

        ACCESS_TOKEN="$NEW_ACCESS_TOKEN"
    else
        print_error "Token refresh failed"
        echo "$REFRESH_OUTPUT"
        exit 1
    fi
else
    print_warning "Skipping refresh test (no refresh token)"
fi

# Step 5: Logout
print_step "Testing Logout (happy path)..."

if [ -n "$REFRESH_TOKEN" ]; then
    LOGOUT_OUTPUT=$(grpcurl -plaintext -v \
        -H "cookie: refresh_token=$REFRESH_TOKEN" \
        -d "{}" \
        "$GRPC_HOST" user.v1.UserService/Logout 2>&1)

    if echo "$LOGOUT_OUTPUT" | grep -q "Response contents:"; then
        print_success "Logout successful"

        # Check if cookie was cleared
        if echo "$LOGOUT_OUTPUT" | grep -i "set-cookie:" | grep -q "Max-Age=0"; then
            print_success "Refresh token cookie cleared"
        fi
    else
        print_warning "Logout response unclear"
    fi
else
    print_warning "Skipping logout test (no refresh token)"
fi

# ============================================
# ERROR TESTS
# ============================================

print_step "Testing Login with wrong password..."
set +e
WRONG_PW_RESPONSE=$(grpcurl -plaintext \
    -d "{\"email\": \"$TEST_EMAIL\", \"password\": \"wrongpassword\"}" \
    "$GRPC_HOST" user.v1.UserService/Login 2>&1)
set -e

if echo "$WRONG_PW_RESPONSE" | grep -qi "unauthenticated\|invalid"; then
    print_success "Wrong password correctly rejected"
else
    print_error "Wrong password should return UNAUTHENTICATED"
    echo "$WRONG_PW_RESPONSE"
    exit 1
fi

print_step "Testing Login with non-existent user..."
set +e
NONEXIST_RESPONSE=$(grpcurl -plaintext \
    -d "{\"email\": \"nonexistent-$(date +%s)@example.com\", \"password\": \"password123\"}" \
    "$GRPC_HOST" user.v1.UserService/Login 2>&1)
set -e

if echo "$NONEXIST_RESPONSE" | grep -qi "unauthenticated\|invalid\|not found"; then
    print_success "Non-existent user correctly rejected"
else
    print_error "Non-existent user should return error"
    echo "$NONEXIST_RESPONSE"
    exit 1
fi

print_step "Testing Refresh with invalid token..."
set +e
INVALID_REFRESH_RESPONSE=$(grpcurl -plaintext \
    -H "cookie: refresh_token=invalid.token.here" \
    -d "{}" \
    "$GRPC_HOST" user.v1.UserService/Refresh 2>&1)
set -e

if echo "$INVALID_REFRESH_RESPONSE" | grep -qi "unauthenticated\|invalid\|expired"; then
    print_success "Invalid refresh token correctly rejected"
else
    print_error "Invalid refresh token should return UNAUTHENTICATED"
    echo "$INVALID_REFRESH_RESPONSE"
    exit 1
fi

print_step "Testing Refresh after logout (token revoked)..."
if [ -n "$REFRESH_TOKEN" ]; then
    set +e
    REVOKED_RESPONSE=$(grpcurl -plaintext \
        -H "cookie: refresh_token=$REFRESH_TOKEN" \
        -d "{}" \
        "$GRPC_HOST" user.v1.UserService/Refresh 2>&1)
    set -e

    if echo "$REVOKED_RESPONSE" | grep -qi "unauthenticated\|not found\|invalid"; then
        print_success "Revoked refresh token correctly rejected"
    else
        print_warning "Revoked token may still be valid (implementation dependent)"
    fi
else
    print_warning "Skipping revoked token test"
fi

print_step "Testing GetUser without token..."
test_unauthenticated "user.v1.UserService/GetUser" "{\"user_id\": \"$USER_ID\"}" "GetUser without token"

print_step "Testing GetUser with invalid token..."
test_invalid_token "user.v1.UserService/GetUser" "{\"user_id\": \"$USER_ID\"}" "GetUser with invalid token"

# ============================================
# SUMMARY
# ============================================
print_summary "Authentication Tests"
echo ""
echo -e "${YELLOW}Test User:${NC} $TEST_EMAIL"
echo -e "${YELLOW}User ID:${NC} $USER_ID"
