#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "Document Tests (DocumentService)"

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
    '{"name": "Document Test Org"}')

ORG_ID=$(echo "$ORG_RESPONSE" | jq -r '.organizationId // .organization.organizationId // empty')
if [ -z "$ORG_ID" ] || [ "$ORG_ID" = "null" ]; then
    print_error "Failed to create organization"
    exit 1
fi
print_success "Organization created: $ORG_ID"

print_step "Setup: Creating workspace..."
WORKSPACE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "workspace.v1.WorkspaceService/CreateWorkspace" \
    "{\"organization_id\": \"$ORG_ID\", \"name\": \"Document Test Workspace\"}")

WORKSPACE_ID=$(echo "$WORKSPACE_RESPONSE" | jq -r '.workspace.id // .id // empty')
if [ -z "$WORKSPACE_ID" ] || [ "$WORKSPACE_ID" = "null" ]; then
    print_error "Failed to create workspace"
    exit 1
fi
print_success "Workspace created: $WORKSPACE_ID"

print_step "Setup: Creating data stream definition..."
DEFINITION_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamDefinitionService/CreateDataStreamDefinition" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Document Test Definition\",
        \"contracts\": [{
            \"match_expression\": \"true\",
            \"transform_expression\": \"cayenne_lpp_decode(input)\",
            \"json_schema\": \"{\\\"type\\\": \\\"object\\\"}\"
        }]
    }")

DEFINITION_ID=$(echo "$DEFINITION_RESPONSE" | jq -r '.dataStreamDefinition.id // .id // empty')
if [ -z "$DEFINITION_ID" ] || [ "$DEFINITION_ID" = "null" ]; then
    print_error "Failed to create definition"
    exit 1
fi
print_success "Definition created: $DEFINITION_ID"

print_step "Setup: Creating gateway..."
GATEWAY_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "gateway.v1.GatewayService/CreateGateway" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"Document Test Gateway\",
        \"type\": \"GATEWAY_TYPE_EMQX\",
        \"emqx_config\": {
            \"broker_url\": \"mqtt://localhost:1883\"
        }
    }")

GATEWAY_ID=$(echo "$GATEWAY_RESPONSE" | jq -r '.gateway.gatewayId // .gatewayId // empty')
if [ -z "$GATEWAY_ID" ] || [ "$GATEWAY_ID" = "null" ]; then
    print_error "Failed to create gateway"
    exit 1
fi
print_success "Gateway created: $GATEWAY_ID"

print_step "Setup: Creating data stream..."
DATA_STREAM_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "data_stream.v1.DataStreamService/CreateDataStream" \
    "{
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"gateway_id\": \"$GATEWAY_ID\",
        \"name\": \"Document Test Data Stream\"
    }")

DATA_STREAM_ID=$(echo "$DATA_STREAM_RESPONSE" | jq -r '.dataStream.dataStreamId // .dataStreamId // empty')
if [ -z "$DATA_STREAM_ID" ] || [ "$DATA_STREAM_ID" = "null" ]; then
    print_error "Failed to create data stream"
    exit 1
fi
print_success "Data stream created: $DATA_STREAM_ID"

# Base64-encoded test content ("Hello, Ponix!" in base64)
TEST_CONTENT=$(echo -n "Hello, Ponix!" | base64)
UPDATED_CONTENT=$(echo -n "Updated content for Ponix!" | base64)

# ============================================
# AGE GRAPH VALIDATION HELPERS
# ============================================
POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-ponix-postgres}"
POSTGRES_USER="${POSTGRES_USER:-ponix}"
POSTGRES_DB="${POSTGRES_DB:-ponix}"

# Query AGE graph for HAS_DOCUMENT edges between a source and document
# Usage: EDGE_COUNT=$(age_edge_count "DataStream" "$source_id" "$doc_id")
age_edge_count() {
    local source_label="$1"
    local source_id="$2"
    local document_id="$3"

    local cypher_query="MATCH (s:${source_label} {id: '${source_id}'})-[e:HAS_DOCUMENT]->(d:Document {id: '${document_id}'}) RETURN count(e)"
    local sql="LOAD 'age'; SET search_path = ag_catalog, \"\$user\", public; SELECT * FROM ag_catalog.cypher('ponix_graph', \$\$ ${cypher_query} \$\$) AS (count ag_catalog.agtype); SET search_path = \"\$user\", public;"

    local result
    result=$(docker exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -t -A -c "$sql" 2>&1)

    # AGE returns agtype values like "1" or "0" — extract the number
    echo "$result" | grep -oE '[0-9]+' | tail -1
}

# Verify AGE edge exists
# Usage: verify_age_edge_exists "DataStream" "$DATA_STREAM_ID" "$DOC_ID" "description"
verify_age_edge_exists() {
    local source_label="$1"
    local source_id="$2"
    local document_id="$3"
    local description="${4:-AGE edge}"

    local count
    count=$(age_edge_count "$source_label" "$source_id" "$document_id")

    if [ -n "$count" ] && [ "$count" -ge 1 ]; then
        print_success "AGE graph: $description edge exists ($source_label -> Document)"
        return 0
    else
        print_error "AGE graph: $description edge NOT found ($source_label {id: $source_id} -> Document {id: $document_id})"
        return 1
    fi
}

# Verify AGE edge does NOT exist
# Usage: verify_age_edge_removed "DataStream" "$DATA_STREAM_ID" "$DOC_ID" "description"
verify_age_edge_removed() {
    local source_label="$1"
    local source_id="$2"
    local document_id="$3"
    local description="${4:-AGE edge}"

    local count
    count=$(age_edge_count "$source_label" "$source_id" "$document_id")

    if [ -z "$count" ] || [ "$count" -eq 0 ]; then
        print_success "AGE graph: $description edge removed ($source_label -> Document)"
        return 0
    else
        print_error "AGE graph: $description edge still exists after unlink ($source_label {id: $source_id} -> Document {id: $document_id}, count: $count)"
        return 1
    fi
}

# Verify we can reach the postgres container for AGE validation
if docker exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "SELECT 1" &>/dev/null; then
    print_success "AGE graph validation ready (container: $POSTGRES_CONTAINER)"
else
    print_error "Cannot reach PostgreSQL container ($POSTGRES_CONTAINER) — required for AGE graph validation"
    exit 1
fi

# ============================================
# DATA STREAM DOCUMENT TESTS
# ============================================

# UploadDataStreamDocument
print_step "Testing UploadDataStreamDocument (happy path)..."

UPLOAD_DS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UploadDataStreamDocument" \
    "{
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"name\": \"ds-sensor-manual.pdf\",
        \"mime_type\": \"application/pdf\",
        \"content\": \"$TEST_CONTENT\"
    }")

DS_DOC_ID=$(echo "$UPLOAD_DS_RESPONSE" | jq -r '.document.documentId // empty')
if [ -n "$DS_DOC_ID" ] && [ "$DS_DOC_ID" != "null" ]; then
    print_success "Data stream document uploaded: $DS_DOC_ID"
else
    print_error "Failed to upload data stream document"
    echo "$UPLOAD_DS_RESPONSE"
    exit 1
fi

# Verify upload response contains expected fields
DS_DOC_NAME=$(echo "$UPLOAD_DS_RESPONSE" | jq -r '.document.name // empty')
DS_DOC_MIME=$(echo "$UPLOAD_DS_RESPONSE" | jq -r '.document.mimeType // empty')
if [ "$DS_DOC_NAME" = "ds-sensor-manual.pdf" ] && [ "$DS_DOC_MIME" = "application/pdf" ]; then
    print_success "Upload response has correct name and mime_type"
else
    print_error "Upload response has unexpected fields: name=$DS_DOC_NAME, mime=$DS_DOC_MIME"
    exit 1
fi

# Verify AGE graph edge created for data stream
print_step "Verifying AGE graph edge: DataStream -> Document..."
verify_age_edge_exists "DataStream" "$DATA_STREAM_ID" "$DS_DOC_ID" "upload data stream"

# ListDataStreamDocuments
print_step "Testing ListDataStreamDocuments (happy path)..."

LIST_DS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/ListDataStreamDocuments" \
    "{
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\"
    }")

DS_DOC_COUNT=$(echo "$LIST_DS_RESPONSE" | jq '.documents | length')
if [ "$DS_DOC_COUNT" -ge 1 ]; then
    print_success "ListDataStreamDocuments returned $DS_DOC_COUNT document(s)"
else
    print_error "ListDataStreamDocuments returned no documents"
    echo "$LIST_DS_RESPONSE"
    exit 1
fi

# Verify list returns DocumentSummary (no checksum field)
HAS_CHECKSUM=$(echo "$LIST_DS_RESPONSE" | jq '.documents[0] | has("checksum")')
if [ "$HAS_CHECKSUM" = "false" ]; then
    print_success "List response uses DocumentSummary (no checksum)"
else
    print_warning "List response may include checksum field"
fi

# GetDocument (standalone)
print_step "Testing GetDocument (happy path)..."

GET_DOC_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/GetDocument" \
    "{
        \"document_id\": \"$DS_DOC_ID\",
        \"organization_id\": \"$ORG_ID\"
    }")

RETURNED_DOC_NAME=$(echo "$GET_DOC_RESPONSE" | jq -r '.document.name // empty')
RETURNED_CHECKSUM=$(echo "$GET_DOC_RESPONSE" | jq -r '.document.checksum // empty')
if [ "$RETURNED_DOC_NAME" = "ds-sensor-manual.pdf" ]; then
    print_success "GetDocument returned correct data"
else
    print_error "GetDocument returned unexpected data"
    echo "$GET_DOC_RESPONSE"
    exit 1
fi

if [ -n "$RETURNED_CHECKSUM" ] && [ "$RETURNED_CHECKSUM" != "null" ]; then
    print_success "GetDocument includes checksum (full Document)"
else
    print_warning "GetDocument missing checksum"
fi

# UpdateDataStreamDocument
print_step "Testing UpdateDataStreamDocument (happy path)..."

UPDATE_DS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UpdateDataStreamDocument" \
    "{
        \"document_id\": \"$DS_DOC_ID\",
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"name\": \"ds-sensor-manual-v2.pdf\"
    }")

UPDATED_DS_NAME=$(echo "$UPDATE_DS_RESPONSE" | jq -r '.document.name // empty')
if [ "$UPDATED_DS_NAME" = "ds-sensor-manual-v2.pdf" ]; then
    print_success "Data stream document updated successfully"
else
    print_error "Data stream document update failed"
    echo "$UPDATE_DS_RESPONSE"
    exit 1
fi

# UpdateDataStreamDocument with content
print_step "Testing UpdateDataStreamDocument with new content..."

UPDATE_DS_CONTENT_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UpdateDataStreamDocument" \
    "{
        \"document_id\": \"$DS_DOC_ID\",
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"content\": \"$UPDATED_CONTENT\"
    }")

UPDATED_SIZE=$(echo "$UPDATE_DS_CONTENT_RESPONSE" | jq -r '.document.sizeBytes // empty')
if [ -n "$UPDATED_SIZE" ] && [ "$UPDATED_SIZE" != "null" ] && [ "$UPDATED_SIZE" != "0" ]; then
    print_success "Document content updated (size: $UPDATED_SIZE bytes)"
else
    print_error "Document content update may have failed"
    echo "$UPDATE_DS_CONTENT_RESPONSE"
    exit 1
fi

# UnlinkDocumentFromDataStream
print_step "Testing UnlinkDocumentFromDataStream (happy path)..."

set +e
UNLINK_DS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UnlinkDocumentFromDataStream" \
    "{
        \"document_id\": \"$DS_DOC_ID\",
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\"
    }" 2>&1)
UNLINK_DS_EXIT=$?
set -e

if [ $UNLINK_DS_EXIT -eq 0 ]; then
    print_success "Document unlinked from data stream"
else
    print_error "Failed to unlink document from data stream"
    echo "$UNLINK_DS_RESPONSE"
    exit 1
fi

# Verify unlink — list should now be empty
LIST_DS_AFTER_UNLINK=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/ListDataStreamDocuments" \
    "{
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\"
    }")

DS_AFTER_COUNT=$(echo "$LIST_DS_AFTER_UNLINK" | jq '.documents | length // 0')
if [ "$DS_AFTER_COUNT" -eq 0 ]; then
    print_success "ListDataStreamDocuments returns empty after unlink"
else
    print_error "ListDataStreamDocuments still returns $DS_AFTER_COUNT document(s) after unlink"
    exit 1
fi

# Verify AGE graph edge removed for data stream
print_step "Verifying AGE graph edge removed: DataStream -> Document..."
verify_age_edge_removed "DataStream" "$DATA_STREAM_ID" "$DS_DOC_ID" "unlink data stream"

# ============================================
# DEFINITION DOCUMENT TESTS
# ============================================

# UploadDefinitionDocument
print_step "Testing UploadDefinitionDocument (happy path)..."

UPLOAD_DEF_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UploadDefinitionDocument" \
    "{
        \"definition_id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"definition-spec.md\",
        \"mime_type\": \"text/markdown\",
        \"content\": \"$TEST_CONTENT\"
    }")

DEF_DOC_ID=$(echo "$UPLOAD_DEF_RESPONSE" | jq -r '.document.documentId // empty')
if [ -n "$DEF_DOC_ID" ] && [ "$DEF_DOC_ID" != "null" ]; then
    print_success "Definition document uploaded: $DEF_DOC_ID"
else
    print_error "Failed to upload definition document"
    echo "$UPLOAD_DEF_RESPONSE"
    exit 1
fi

# Verify AGE graph edge created for definition
print_step "Verifying AGE graph edge: DataStreamDefinition -> Document..."
verify_age_edge_exists "DataStreamDefinition" "$DEFINITION_ID" "$DEF_DOC_ID" "upload definition"

# ListDefinitionDocuments
print_step "Testing ListDefinitionDocuments (happy path)..."

LIST_DEF_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/ListDefinitionDocuments" \
    "{
        \"definition_id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\"
    }")

DEF_DOC_COUNT=$(echo "$LIST_DEF_RESPONSE" | jq '.documents | length')
if [ "$DEF_DOC_COUNT" -ge 1 ]; then
    print_success "ListDefinitionDocuments returned $DEF_DOC_COUNT document(s)"
else
    print_error "ListDefinitionDocuments returned no documents"
    echo "$LIST_DEF_RESPONSE"
    exit 1
fi

# UpdateDefinitionDocument
print_step "Testing UpdateDefinitionDocument (happy path)..."

UPDATE_DEF_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UpdateDefinitionDocument" \
    "{
        \"document_id\": \"$DEF_DOC_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"definition-spec-v2.md\"
    }")

UPDATED_DEF_NAME=$(echo "$UPDATE_DEF_RESPONSE" | jq -r '.document.name // empty')
if [ "$UPDATED_DEF_NAME" = "definition-spec-v2.md" ]; then
    print_success "Definition document updated successfully"
else
    print_error "Definition document update failed"
    echo "$UPDATE_DEF_RESPONSE"
    exit 1
fi

# UnlinkDocumentFromDefinition
print_step "Testing UnlinkDocumentFromDefinition (happy path)..."

set +e
UNLINK_DEF_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UnlinkDocumentFromDefinition" \
    "{
        \"document_id\": \"$DEF_DOC_ID\",
        \"definition_id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\"
    }" 2>&1)
UNLINK_DEF_EXIT=$?
set -e

if [ $UNLINK_DEF_EXIT -eq 0 ]; then
    print_success "Document unlinked from definition"
else
    print_error "Failed to unlink document from definition"
    echo "$UNLINK_DEF_RESPONSE"
    exit 1
fi

# Verify unlink
LIST_DEF_AFTER_UNLINK=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/ListDefinitionDocuments" \
    "{
        \"definition_id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\"
    }")

DEF_AFTER_COUNT=$(echo "$LIST_DEF_AFTER_UNLINK" | jq '.documents | length // 0')
if [ "$DEF_AFTER_COUNT" -eq 0 ]; then
    print_success "ListDefinitionDocuments returns empty after unlink"
else
    print_error "ListDefinitionDocuments still returns $DEF_AFTER_COUNT document(s) after unlink"
    exit 1
fi

# Verify AGE graph edge removed for definition
print_step "Verifying AGE graph edge removed: DataStreamDefinition -> Document..."
verify_age_edge_removed "DataStreamDefinition" "$DEFINITION_ID" "$DEF_DOC_ID" "unlink definition"

# ============================================
# WORKSPACE DOCUMENT TESTS
# ============================================

# UploadWorkspaceDocument
print_step "Testing UploadWorkspaceDocument (happy path)..."

UPLOAD_WS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UploadWorkspaceDocument" \
    "{
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"workspace-notes.txt\",
        \"mime_type\": \"text/plain\",
        \"content\": \"$TEST_CONTENT\"
    }")

WS_DOC_ID=$(echo "$UPLOAD_WS_RESPONSE" | jq -r '.document.documentId // empty')
if [ -n "$WS_DOC_ID" ] && [ "$WS_DOC_ID" != "null" ]; then
    print_success "Workspace document uploaded: $WS_DOC_ID"
else
    print_error "Failed to upload workspace document"
    echo "$UPLOAD_WS_RESPONSE"
    exit 1
fi

# Verify AGE graph edge created for workspace
print_step "Verifying AGE graph edge: Workspace -> Document..."
verify_age_edge_exists "Workspace" "$WORKSPACE_ID" "$WS_DOC_ID" "upload workspace"

# ListWorkspaceDocuments
print_step "Testing ListWorkspaceDocuments (happy path)..."

LIST_WS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/ListWorkspaceDocuments" \
    "{
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\"
    }")

WS_DOC_COUNT=$(echo "$LIST_WS_RESPONSE" | jq '.documents | length')
if [ "$WS_DOC_COUNT" -ge 1 ]; then
    print_success "ListWorkspaceDocuments returned $WS_DOC_COUNT document(s)"
else
    print_error "ListWorkspaceDocuments returned no documents"
    echo "$LIST_WS_RESPONSE"
    exit 1
fi

# UpdateWorkspaceDocument
print_step "Testing UpdateWorkspaceDocument (happy path)..."

UPDATE_WS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UpdateWorkspaceDocument" \
    "{
        \"document_id\": \"$WS_DOC_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"workspace-notes-v2.txt\"
    }")

UPDATED_WS_NAME=$(echo "$UPDATE_WS_RESPONSE" | jq -r '.document.name // empty')
if [ "$UPDATED_WS_NAME" = "workspace-notes-v2.txt" ]; then
    print_success "Workspace document updated successfully"
else
    print_error "Workspace document update failed"
    echo "$UPDATE_WS_RESPONSE"
    exit 1
fi

# UnlinkDocumentFromWorkspace
print_step "Testing UnlinkDocumentFromWorkspace (happy path)..."

set +e
UNLINK_WS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UnlinkDocumentFromWorkspace" \
    "{
        \"document_id\": \"$WS_DOC_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\"
    }" 2>&1)
UNLINK_WS_EXIT=$?
set -e

if [ $UNLINK_WS_EXIT -eq 0 ]; then
    print_success "Document unlinked from workspace"
else
    print_error "Failed to unlink document from workspace"
    echo "$UNLINK_WS_RESPONSE"
    exit 1
fi

# Verify unlink
LIST_WS_AFTER_UNLINK=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/ListWorkspaceDocuments" \
    "{
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\"
    }")

WS_AFTER_COUNT=$(echo "$LIST_WS_AFTER_UNLINK" | jq '.documents | length // 0')
if [ "$WS_AFTER_COUNT" -eq 0 ]; then
    print_success "ListWorkspaceDocuments returns empty after unlink"
else
    print_error "ListWorkspaceDocuments still returns $WS_AFTER_COUNT document(s) after unlink"
    exit 1
fi

# Verify AGE graph edge removed for workspace
print_step "Verifying AGE graph edge removed: Workspace -> Document..."
verify_age_edge_removed "Workspace" "$WORKSPACE_ID" "$WS_DOC_ID" "unlink workspace"

# ============================================
# DELETE DOCUMENT TEST
# ============================================

# Upload a document to delete
print_step "Testing DeleteDocument (happy path)..."

UPLOAD_DELETE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UploadWorkspaceDocument" \
    "{
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"to-delete.txt\",
        \"mime_type\": \"text/plain\",
        \"content\": \"$TEST_CONTENT\"
    }")

DELETE_DOC_ID=$(echo "$UPLOAD_DELETE_RESPONSE" | jq -r '.document.documentId // empty')
if [ -z "$DELETE_DOC_ID" ] || [ "$DELETE_DOC_ID" = "null" ]; then
    print_error "Failed to upload document for delete test"
    exit 1
fi

set +e
DELETE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/DeleteDocument" \
    "{
        \"document_id\": \"$DELETE_DOC_ID\",
        \"organization_id\": \"$ORG_ID\"
    }" 2>&1)
DELETE_EXIT=$?
set -e

if [ $DELETE_EXIT -eq 0 ]; then
    print_success "Document deleted successfully"
else
    print_error "Failed to delete document"
    echo "$DELETE_RESPONSE"
    exit 1
fi

# Verify deleted document returns not found
set +e
GET_DELETED=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/GetDocument" \
    "{
        \"document_id\": \"$DELETE_DOC_ID\",
        \"organization_id\": \"$ORG_ID\"
    }" 2>&1)
set -e

if echo "$GET_DELETED" | grep -qi "not found\|does not exist"; then
    print_success "Deleted document returns not found"
else
    print_warning "Deleted document may be soft-deleted (still accessible)"
fi

# ============================================
# UPLOAD WITH METADATA TEST
# ============================================

print_step "Testing upload with metadata..."

UPLOAD_META_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UploadWorkspaceDocument" \
    "{
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"with-metadata.json\",
        \"mime_type\": \"application/json\",
        \"metadata\": {\"fields\": {\"author\": {\"stringValue\": \"test-user\"}, \"version\": {\"numberValue\": 1}}},
        \"content\": \"$TEST_CONTENT\"
    }")

META_DOC_ID=$(echo "$UPLOAD_META_RESPONSE" | jq -r '.document.documentId // empty')
if [ -n "$META_DOC_ID" ] && [ "$META_DOC_ID" != "null" ]; then
    print_success "Document with metadata uploaded: $META_DOC_ID"
else
    print_error "Failed to upload document with metadata"
    echo "$UPLOAD_META_RESPONSE"
    exit 1
fi

# Verify metadata via GetDocument
GET_META_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/GetDocument" \
    "{
        \"document_id\": \"$META_DOC_ID\",
        \"organization_id\": \"$ORG_ID\"
    }")

HAS_META=$(echo "$GET_META_RESPONSE" | jq '.document.metadata // null')
if [ "$HAS_META" != "null" ]; then
    print_success "Document metadata preserved"
else
    print_warning "Document metadata may not have been stored"
fi

# ============================================
# UNAUTHENTICATED TESTS
# ============================================

print_step "Testing GetDocument without auth..."
test_unauthenticated \
    "document.v1.DocumentService/GetDocument" \
    "{\"document_id\": \"$DS_DOC_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing DeleteDocument without auth..."
test_unauthenticated \
    "document.v1.DocumentService/DeleteDocument" \
    "{\"document_id\": \"$DS_DOC_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing UploadDataStreamDocument without auth..."
test_unauthenticated \
    "document.v1.DocumentService/UploadDataStreamDocument" \
    "{\"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\", \"name\": \"unauthorized.txt\", \"mime_type\": \"text/plain\", \"content\": \"$TEST_CONTENT\"}"

print_step "Testing ListDataStreamDocuments without auth..."
test_unauthenticated \
    "document.v1.DocumentService/ListDataStreamDocuments" \
    "{\"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}"

print_step "Testing UnlinkDocumentFromDataStream without auth..."
test_unauthenticated \
    "document.v1.DocumentService/UnlinkDocumentFromDataStream" \
    "{\"document_id\": \"$DS_DOC_ID\", \"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}"

print_step "Testing UploadDefinitionDocument without auth..."
test_unauthenticated \
    "document.v1.DocumentService/UploadDefinitionDocument" \
    "{\"definition_id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"unauthorized.txt\", \"mime_type\": \"text/plain\", \"content\": \"$TEST_CONTENT\"}"

print_step "Testing ListDefinitionDocuments without auth..."
test_unauthenticated \
    "document.v1.DocumentService/ListDefinitionDocuments" \
    "{\"definition_id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing UploadWorkspaceDocument without auth..."
test_unauthenticated \
    "document.v1.DocumentService/UploadWorkspaceDocument" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"unauthorized.txt\", \"mime_type\": \"text/plain\", \"content\": \"$TEST_CONTENT\"}"

print_step "Testing ListWorkspaceDocuments without auth..."
test_unauthenticated \
    "document.v1.DocumentService/ListWorkspaceDocuments" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\"}"

# ============================================
# INVALID TOKEN TESTS
# ============================================

print_step "Testing GetDocument with invalid token..."
test_invalid_token \
    "document.v1.DocumentService/GetDocument" \
    "{\"document_id\": \"$DS_DOC_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing UploadDataStreamDocument with invalid token..."
test_invalid_token \
    "document.v1.DocumentService/UploadDataStreamDocument" \
    "{\"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\", \"name\": \"invalid.txt\", \"mime_type\": \"text/plain\", \"content\": \"$TEST_CONTENT\"}"

print_step "Testing UploadDefinitionDocument with invalid token..."
test_invalid_token \
    "document.v1.DocumentService/UploadDefinitionDocument" \
    "{\"definition_id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"invalid.txt\", \"mime_type\": \"text/plain\", \"content\": \"$TEST_CONTENT\"}"

print_step "Testing UploadWorkspaceDocument with invalid token..."
test_invalid_token \
    "document.v1.DocumentService/UploadWorkspaceDocument" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"invalid.txt\", \"mime_type\": \"text/plain\", \"content\": \"$TEST_CONTENT\"}"

# ============================================
# SUMMARY
# ============================================
print_summary "Document Tests"
echo ""
echo -e "${YELLOW}Test Organization ID:${NC} $ORG_ID"
echo -e "${YELLOW}Test Workspace ID:${NC} $WORKSPACE_ID"
echo -e "${YELLOW}Test Definition ID:${NC} $DEFINITION_ID"
echo -e "${YELLOW}Test Data Stream ID:${NC} $DATA_STREAM_ID"
echo -e "${YELLOW}Test DS Document ID:${NC} $DS_DOC_ID"
echo -e "${YELLOW}Test Def Document ID:${NC} $DEF_DOC_ID"
echo -e "${YELLOW}Test WS Document ID:${NC} $WS_DOC_ID"

# Export for dependent tests
export ORG_ID WORKSPACE_ID DEFINITION_ID DATA_STREAM_ID AUTH_TOKEN
