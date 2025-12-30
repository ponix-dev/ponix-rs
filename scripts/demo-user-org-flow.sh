#!/bin/bash

# Demo script for user registration, login, organization creation, and listing
# Requires: grpcurl, jq
# Usage: ./scripts/demo-user-org-flow.sh [host:port]

set -e

HOST="${1:-localhost:50051}"
echo "Using gRPC endpoint: $HOST"
echo "================================"

# 1. Register a new user
echo ""
echo "1. Registering new user..."
REGISTER_RESPONSE=$(grpcurl -plaintext -d '{
  "email": "demo@example.com",
  "password": "securepassword123",
  "name": "Demo User"
}' "$HOST" user.v1.UserService/RegisterUser 2>&1) || true

echo "$REGISTER_RESPONSE"

# 2. Login to get access token
echo ""
echo "2. Logging in..."
LOGIN_RESPONSE=$(grpcurl -plaintext -d '{
  "email": "demo@example.com",
  "password": "securepassword123"
}' "$HOST" user.v1.UserService/Login)

echo "$LOGIN_RESPONSE"

# Extract access token
ACCESS_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token')

if [ "$ACCESS_TOKEN" == "null" ] || [ -z "$ACCESS_TOKEN" ]; then
  echo "ERROR: Failed to get access token"
  exit 1
fi

# Decode JWT to get user_id from 'sub' claim
# JWT format: header.payload.signature - we need the payload (2nd part)
JWT_PAYLOAD=$(echo "$ACCESS_TOKEN" | cut -d'.' -f2)
# Add padding if needed for base64 decode
PADDED_PAYLOAD="$JWT_PAYLOAD$(printf '%s' '==' | head -c $((4 - ${#JWT_PAYLOAD} % 4)))"
USER_ID=$(echo "$PADDED_PAYLOAD" | base64 -d 2>/dev/null | jq -r '.sub')

echo ""
echo "Got access token: ${ACCESS_TOKEN:0:50}..."
echo "User ID: $USER_ID"

# 3. Create an organization
echo ""
echo "3. Creating organization..."
CREATE_ORG_RESPONSE=$(grpcurl -plaintext \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -d '{
    "name": "My Demo Organization"
  }' "$HOST" organization.v1.OrganizationService/CreateOrganization)

echo "$CREATE_ORG_RESPONSE"

ORG_ID=$(echo "$CREATE_ORG_RESPONSE" | jq -r '.organizationId')
echo ""
echo "Created organization ID: $ORG_ID"

# 4. Get user's organizations
echo ""
echo "4. Listing user's organizations..."
USER_ORGS_RESPONSE=$(grpcurl -plaintext \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -d "{
    \"user_id\": \"$USER_ID\"
  }" "$HOST" organization.v1.OrganizationService/UserOrganizations)

echo "$USER_ORGS_RESPONSE"

# 5. Try to access another user's organizations (should fail with PERMISSION_DENIED)
echo ""
echo "5. Testing authorization - trying to access another user's orgs (should fail)..."
UNAUTHORIZED_RESPONSE=$(grpcurl -plaintext \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -d '{
    "user_id": "some-other-user-id"
  }' "$HOST" organization.v1.OrganizationService/UserOrganizations 2>&1) || true

echo "$UNAUTHORIZED_RESPONSE"

echo ""
echo "================================"
echo "Demo complete!"
