#!/bin/bash

# Test script for user registration, login, refresh, and logout
# Requires: grpcurl (brew install grpcurl)

set -e

HOST="${GRPC_HOST:-localhost:50051}"
EMAIL="${TEST_EMAIL:-test@example.com}"
PASSWORD="${TEST_PASSWORD:-securepassword123}"
NAME="${TEST_NAME:-Test User}"

echo "=== User Authentication Flow Test ==="
echo "Host: $HOST"
echo "Email: $EMAIL"
echo ""

# Register user
echo "1. Registering user..."
REGISTER_RESPONSE=$(grpcurl -plaintext \
  -d "{\"email\": \"$EMAIL\", \"password\": \"$PASSWORD\", \"name\": \"$NAME\"}" \
  "$HOST" user.v1.UserService/RegisterUser 2>&1) || true

if echo "$REGISTER_RESPONSE" | grep -q "already exists"; then
  echo "   User already exists, proceeding to login..."
elif echo "$REGISTER_RESPONSE" | grep -q "error"; then
  echo "   Registration response: $REGISTER_RESPONSE"
else
  echo "   Registration successful!"
  echo "$REGISTER_RESPONSE" | jq . 2>/dev/null || echo "$REGISTER_RESPONSE"
fi
echo ""

# Login (capture headers to get refresh token cookie)
echo "2. Logging in..."
LOGIN_OUTPUT=$(grpcurl -plaintext -v \
  -d "{\"email\": \"$EMAIL\", \"password\": \"$PASSWORD\"}" \
  "$HOST" user.v1.UserService/Login 2>&1)

# Extract the response body (JSON part) - use sed to get content between braces
LOGIN_RESPONSE=$(echo "$LOGIN_OUTPUT" | sed -n '/^{/,/^}/p')

echo "   Login successful!"
echo "$LOGIN_RESPONSE" | jq . 2>/dev/null || echo "$LOGIN_RESPONSE"

# Extract access token
ACCESS_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token' 2>/dev/null)
if [ -n "$ACCESS_TOKEN" ] && [ "$ACCESS_TOKEN" != "null" ]; then
  echo ""
  echo "3. JWT Access Token received:"
  echo "   ${ACCESS_TOKEN:0:50}..."
  echo ""
  echo "   Decode at: https://jwt.io/#debugger-io?token=$ACCESS_TOKEN"
fi

# Extract refresh token from set-cookie header
# grpcurl verbose output shows headers in format: "header-name: value"
# The set-cookie header contains: refresh_token=TOKEN; Max-Age=...; Path=...; HttpOnly; SameSite=None
REFRESH_TOKEN=""
SET_COOKIE_LINE=$(echo "$LOGIN_OUTPUT" | grep -i "set-cookie")
if [ -n "$SET_COOKIE_LINE" ]; then
  # Extract the token value between "refresh_token=" and the first ";"
  REFRESH_TOKEN=$(echo "$SET_COOKIE_LINE" | sed -n 's/.*refresh_token=\([^;]*\).*/\1/p' | head -1)
fi

if [ -n "$REFRESH_TOKEN" ]; then
  echo ""
  echo "4. Refresh Token cookie received:"
  echo "   ${REFRESH_TOKEN:0:50}..."
else
  echo ""
  echo "4. WARNING: No refresh token cookie found in response headers"
  echo "   Debug: Looking for set-cookie in response..."
  echo "$LOGIN_OUTPUT" | grep -i -E "(set-cookie|Response headers)" | head -5 || echo "   No headers found"
fi
echo ""

# Test token refresh
echo "5. Testing token refresh..."
if [ -n "$REFRESH_TOKEN" ] && [ "$REFRESH_TOKEN" != "" ]; then
  REFRESH_OUTPUT=$(grpcurl -plaintext -v \
    -H "cookie: refresh_token=$REFRESH_TOKEN" \
    -d "{}" \
    "$HOST" user.v1.UserService/Refresh 2>&1) || true

  # Check for successful response (accessToken in camelCase or access_token in snake_case)
  if echo "$REFRESH_OUTPUT" | grep -q -i "accesstoken\|access_token"; then
    REFRESH_RESPONSE=$(echo "$REFRESH_OUTPUT" | sed -n '/^{/,/^}/p')
    NEW_ACCESS_TOKEN=$(echo "$REFRESH_RESPONSE" | jq -r '.accessToken // .access_token' 2>/dev/null)
    echo "   Token refresh successful!"
    echo "   New access token: ${NEW_ACCESS_TOKEN:0:50}..."

    # Extract new refresh token (should be rotated)
    SET_COOKIE_LINE=$(echo "$REFRESH_OUTPUT" | grep -i "set-cookie")
    if [ -n "$SET_COOKIE_LINE" ]; then
      NEW_REFRESH_TOKEN=$(echo "$SET_COOKIE_LINE" | sed -n 's/.*refresh_token=\([^;]*\).*/\1/p' | head -1)
      if [ -n "$NEW_REFRESH_TOKEN" ] && [ "$NEW_REFRESH_TOKEN" != "$REFRESH_TOKEN" ]; then
        echo "   Refresh token rotated successfully!"
        REFRESH_TOKEN="$NEW_REFRESH_TOKEN"
      fi
    fi
  elif echo "$REFRESH_OUTPUT" | grep -q "UNAUTHENTICATED\|expired\|Invalid"; then
    echo "   Refresh failed (token may be invalid or expired)"
    echo "   Response: $(echo "$REFRESH_OUTPUT" | grep -i "desc\|message" | head -1)"
  else
    echo "   Refresh response: $REFRESH_OUTPUT"
  fi
else
  echo "   Skipping (no refresh token available)"
fi
echo ""

# Test invalid login
echo "6. Testing invalid credentials..."
INVALID_RESPONSE=$(grpcurl -plaintext \
  -d "{\"email\": \"$EMAIL\", \"password\": \"wrongpassword\"}" \
  "$HOST" user.v1.UserService/Login 2>&1) || true

if echo "$INVALID_RESPONSE" | grep -q "UNAUTHENTICATED\|Invalid"; then
  echo "   Correctly rejected invalid password!"
else
  echo "   Response: $INVALID_RESPONSE"
fi
echo ""

# Test logout
echo "7. Testing logout..."
if [ -n "$REFRESH_TOKEN" ] && [ "$REFRESH_TOKEN" != "" ]; then
  LOGOUT_OUTPUT=$(grpcurl -plaintext -v \
    -H "cookie: refresh_token=$REFRESH_TOKEN" \
    -d "{}" \
    "$HOST" user.v1.UserService/Logout 2>&1) || true

  if echo "$LOGOUT_OUTPUT" | grep -q "Response contents:"; then
    echo "   Logout successful!"

    # Check if cookie was cleared
    if echo "$LOGOUT_OUTPUT" | grep -i "set-cookie:" | grep -q "Max-Age=0"; then
      echo "   Refresh token cookie cleared!"
    fi
  else
    echo "   Logout response: $LOGOUT_OUTPUT"
  fi

  # Verify the old refresh token no longer works
  echo ""
  echo "8. Verifying old refresh token is invalidated..."
  VERIFY_OUTPUT=$(grpcurl -plaintext \
    -H "cookie: refresh_token=$REFRESH_TOKEN" \
    -d "{}" \
    "$HOST" user.v1.UserService/Refresh 2>&1) || true

  if echo "$VERIFY_OUTPUT" | grep -q "UNAUTHENTICATED\|not found\|Invalid"; then
    echo "   Old refresh token correctly rejected!"
  else
    echo "   WARNING: Old refresh token may still be valid"
    echo "   Response: $VERIFY_OUTPUT"
  fi
else
  echo "   Skipping (no refresh token available)"
fi

echo ""
echo "=== Test Complete ==="
