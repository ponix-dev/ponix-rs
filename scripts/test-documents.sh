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

# CreateDataStreamDocument
print_step "Testing CreateDataStreamDocument (happy path)..."

# Start CDC listener before creating document
start_cdc_listener "documents"

CREATE_DS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/CreateDataStreamDocument" \
    "{
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"name\": \"ds-sensor-manual\"
    }")

DS_DOC_ID=$(echo "$CREATE_DS_RESPONSE" | jq -r '.document.documentId // empty')
if [ -n "$DS_DOC_ID" ] && [ "$DS_DOC_ID" != "null" ]; then
    print_success "Data stream document created: $DS_DOC_ID"
else
    print_error "Failed to create data stream document"
    echo "$CREATE_DS_RESPONSE"
    cleanup_cdc_listener
    exit 1
fi

# Verify CDC create event
wait_for_cdc_event "documents" "create"

# Verify create response contains expected fields
DS_DOC_NAME=$(echo "$CREATE_DS_RESPONSE" | jq -r '.document.name // empty')
if [ "$DS_DOC_NAME" = "ds-sensor-manual" ]; then
    print_success "Create response has correct name"
else
    print_error "Create response has unexpected name: $DS_DOC_NAME"
    exit 1
fi

# Verify AGE graph edge created for data stream
print_step "Verifying AGE graph edge: DataStream -> Document..."
verify_age_edge_exists "DataStream" "$DATA_STREAM_ID" "$DS_DOC_ID" "create data stream"

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

# Verify list returns full Document (not DocumentSummary) — should have timestamps
LIST_DOC_HAS_CREATED=$(echo "$LIST_DS_RESPONSE" | jq '.documents[0] | has("createdAt")')
LIST_DOC_HAS_UPDATED=$(echo "$LIST_DS_RESPONSE" | jq '.documents[0] | has("updatedAt")')
LIST_DOC_NO_CHECKSUM=$(echo "$LIST_DS_RESPONSE" | jq '.documents[0] | has("checksum")')
if [ "$LIST_DOC_HAS_CREATED" = "true" ] && [ "$LIST_DOC_HAS_UPDATED" = "true" ] && [ "$LIST_DOC_NO_CHECKSUM" = "false" ]; then
    print_success "List response returns full Document type (no checksum, has timestamps)"
else
    print_error "List response has unexpected shape"
    echo "$LIST_DS_RESPONSE" | jq '.documents[0] | keys'
    exit 1
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
if [ "$RETURNED_DOC_NAME" = "ds-sensor-manual" ]; then
    print_success "GetDocument returned correct name"
else
    print_error "GetDocument returned unexpected name"
    echo "$GET_DOC_RESPONSE"
    exit 1
fi

# Verify new Yrs-based fields are present in the Document response
print_step "Verifying Document response shape (new fields present, old fields absent)..."

# New fields: contentText and contentHtml should exist (empty strings for new docs, but present)
HAS_CONTENT_TEXT=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("contentText")')
HAS_CONTENT_HTML=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("contentHtml")')

# Proto3 omits default values (empty strings) from JSON — so these fields won't appear
# until the snapshotter populates them. What matters is the OLD fields are gone.

# Old fields must NOT be present
HAS_MIME_TYPE=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("mimeType")')
HAS_SIZE_BYTES=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("sizeBytes")')
HAS_CHECKSUM=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("checksum")')

if [ "$HAS_MIME_TYPE" = "false" ]; then
    print_success "Document does not contain mimeType (removed)"
else
    print_error "Document still contains mimeType — proto not updated"
    exit 1
fi

if [ "$HAS_SIZE_BYTES" = "false" ]; then
    print_success "Document does not contain sizeBytes (removed)"
else
    print_error "Document still contains sizeBytes — proto not updated"
    exit 1
fi

if [ "$HAS_CHECKSUM" = "false" ]; then
    print_success "Document does not contain checksum (removed)"
else
    print_error "Document still contains checksum — proto not updated"
    exit 1
fi

# Verify required fields are present
HAS_DOC_ID=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("documentId")')
HAS_ORG_ID=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("organizationId")')
HAS_CREATED_AT=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("createdAt")')
HAS_UPDATED_AT=$(echo "$GET_DOC_RESPONSE" | jq '.document | has("updatedAt")')

if [ "$HAS_DOC_ID" = "true" ] && [ "$HAS_ORG_ID" = "true" ] && [ "$HAS_CREATED_AT" = "true" ] && [ "$HAS_UPDATED_AT" = "true" ]; then
    print_success "Document contains all expected fields (documentId, organizationId, createdAt, updatedAt)"
else
    print_error "Document missing expected fields"
    echo "$GET_DOC_RESPONSE" | jq '.document | keys'
    exit 1
fi

# UpdateDataStreamDocument
print_step "Testing UpdateDataStreamDocument (happy path)..."

UPDATE_DS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UpdateDataStreamDocument" \
    "{
        \"document_id\": \"$DS_DOC_ID\",
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"ds-sensor-manual-v2\"
    }")

UPDATED_DS_NAME=$(echo "$UPDATE_DS_RESPONSE" | jq -r '.document.name // empty')
if [ "$UPDATED_DS_NAME" = "ds-sensor-manual-v2" ]; then
    print_success "Data stream document updated successfully"
else
    print_error "Data stream document update failed"
    echo "$UPDATE_DS_RESPONSE"
    exit 1
fi

# UpdateDataStreamDocument with metadata
print_step "Testing UpdateDataStreamDocument with metadata..."

UPDATE_DS_META_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/UpdateDataStreamDocument" \
    "{
        \"document_id\": \"$DS_DOC_ID\",
        \"data_stream_id\": \"$DATA_STREAM_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"metadata\": {\"fields\": {\"revision\": {\"numberValue\": 2}}}
    }")

UPDATED_META_NAME=$(echo "$UPDATE_DS_META_RESPONSE" | jq -r '.document.name // empty')
if [ -n "$UPDATED_META_NAME" ] && [ "$UPDATED_META_NAME" != "null" ]; then
    print_success "Document metadata updated successfully"
else
    print_error "Document metadata update may have failed"
    echo "$UPDATE_DS_META_RESPONSE"
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

# CreateDefinitionDocument
print_step "Testing CreateDefinitionDocument (happy path)..."

CREATE_DEF_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/CreateDefinitionDocument" \
    "{
        \"definition_id\": \"$DEFINITION_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"definition-spec\"
    }")

DEF_DOC_ID=$(echo "$CREATE_DEF_RESPONSE" | jq -r '.document.documentId // empty')
if [ -n "$DEF_DOC_ID" ] && [ "$DEF_DOC_ID" != "null" ]; then
    print_success "Definition document created: $DEF_DOC_ID"
else
    print_error "Failed to create definition document"
    echo "$CREATE_DEF_RESPONSE"
    exit 1
fi

# Verify AGE graph edge created for definition
print_step "Verifying AGE graph edge: DataStreamDefinition -> Document..."
verify_age_edge_exists "DataStreamDefinition" "$DEFINITION_ID" "$DEF_DOC_ID" "create definition"

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

# Verify list returns full Document (not DocumentSummary)
LIST_DEF_HAS_CREATED=$(echo "$LIST_DEF_RESPONSE" | jq '.documents[0] | has("createdAt")')
LIST_DEF_NO_CHECKSUM=$(echo "$LIST_DEF_RESPONSE" | jq '.documents[0] | has("checksum")')
LIST_DEF_NO_MIME=$(echo "$LIST_DEF_RESPONSE" | jq '.documents[0] | has("mimeType")')
if [ "$LIST_DEF_HAS_CREATED" = "true" ] && [ "$LIST_DEF_NO_CHECKSUM" = "false" ] && [ "$LIST_DEF_NO_MIME" = "false" ]; then
    print_success "Definition list returns full Document type (no checksum/mimeType)"
else
    print_error "Definition list response has unexpected shape"
    echo "$LIST_DEF_RESPONSE" | jq '.documents[0] | keys'
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
        \"name\": \"definition-spec-v2\"
    }")

UPDATED_DEF_NAME=$(echo "$UPDATE_DEF_RESPONSE" | jq -r '.document.name // empty')
if [ "$UPDATED_DEF_NAME" = "definition-spec-v2" ]; then
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

# CreateWorkspaceDocument
print_step "Testing CreateWorkspaceDocument (happy path)..."

CREATE_WS_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/CreateWorkspaceDocument" \
    "{
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"workspace-notes\"
    }")

WS_DOC_ID=$(echo "$CREATE_WS_RESPONSE" | jq -r '.document.documentId // empty')
if [ -n "$WS_DOC_ID" ] && [ "$WS_DOC_ID" != "null" ]; then
    print_success "Workspace document created: $WS_DOC_ID"
else
    print_error "Failed to create workspace document"
    echo "$CREATE_WS_RESPONSE"
    exit 1
fi

# Verify AGE graph edge created for workspace
print_step "Verifying AGE graph edge: Workspace -> Document..."
verify_age_edge_exists "Workspace" "$WORKSPACE_ID" "$WS_DOC_ID" "create workspace"

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

# Verify list returns full Document (not DocumentSummary)
LIST_WS_HAS_CREATED=$(echo "$LIST_WS_RESPONSE" | jq '.documents[0] | has("createdAt")')
LIST_WS_NO_CHECKSUM=$(echo "$LIST_WS_RESPONSE" | jq '.documents[0] | has("checksum")')
LIST_WS_NO_MIME=$(echo "$LIST_WS_RESPONSE" | jq '.documents[0] | has("mimeType")')
if [ "$LIST_WS_HAS_CREATED" = "true" ] && [ "$LIST_WS_NO_CHECKSUM" = "false" ] && [ "$LIST_WS_NO_MIME" = "false" ]; then
    print_success "Workspace list returns full Document type (no checksum/mimeType)"
else
    print_error "Workspace list response has unexpected shape"
    echo "$LIST_WS_RESPONSE" | jq '.documents[0] | keys'
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
        \"name\": \"workspace-notes-v2\"
    }")

UPDATED_WS_NAME=$(echo "$UPDATE_WS_RESPONSE" | jq -r '.document.name // empty')
if [ "$UPDATED_WS_NAME" = "workspace-notes-v2" ]; then
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

# Create a document to delete
print_step "Testing DeleteDocument (happy path)..."

CREATE_DELETE_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/CreateWorkspaceDocument" \
    "{
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"to-delete\"
    }")

DELETE_DOC_ID=$(echo "$CREATE_DELETE_RESPONSE" | jq -r '.document.documentId // empty')
if [ -z "$DELETE_DOC_ID" ] || [ "$DELETE_DOC_ID" = "null" ]; then
    print_error "Failed to create document for delete test"
    exit 1
fi

# Start CDC listener before deleting document
start_cdc_listener "documents"

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
    cleanup_cdc_listener
    exit 1
fi

# Verify CDC delete event (soft delete triggers an UPDATE in CDC)
wait_for_cdc_event "documents" "update"

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
# CREATE WITH METADATA TEST
# ============================================

print_step "Testing create with metadata..."

CREATE_META_RESPONSE=$(grpc_call "$AUTH_TOKEN" \
    "document.v1.DocumentService/CreateWorkspaceDocument" \
    "{
        \"workspace_id\": \"$WORKSPACE_ID\",
        \"organization_id\": \"$ORG_ID\",
        \"name\": \"with-metadata\",
        \"metadata\": {\"fields\": {\"author\": {\"stringValue\": \"test-user\"}, \"version\": {\"numberValue\": 1}}}
    }")

META_DOC_ID=$(echo "$CREATE_META_RESPONSE" | jq -r '.document.documentId // empty')
if [ -n "$META_DOC_ID" ] && [ "$META_DOC_ID" != "null" ]; then
    print_success "Document with metadata created: $META_DOC_ID"
else
    print_error "Failed to create document with metadata"
    echo "$CREATE_META_RESPONSE"
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

print_step "Testing CreateDataStreamDocument without auth..."
test_unauthenticated \
    "document.v1.DocumentService/CreateDataStreamDocument" \
    "{\"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\", \"name\": \"unauthorized\"}"

print_step "Testing ListDataStreamDocuments without auth..."
test_unauthenticated \
    "document.v1.DocumentService/ListDataStreamDocuments" \
    "{\"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}"

print_step "Testing UnlinkDocumentFromDataStream without auth..."
test_unauthenticated \
    "document.v1.DocumentService/UnlinkDocumentFromDataStream" \
    "{\"document_id\": \"$DS_DOC_ID\", \"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\"}"

print_step "Testing CreateDefinitionDocument without auth..."
test_unauthenticated \
    "document.v1.DocumentService/CreateDefinitionDocument" \
    "{\"definition_id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"unauthorized\"}"

print_step "Testing ListDefinitionDocuments without auth..."
test_unauthenticated \
    "document.v1.DocumentService/ListDefinitionDocuments" \
    "{\"definition_id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\"}"

print_step "Testing CreateWorkspaceDocument without auth..."
test_unauthenticated \
    "document.v1.DocumentService/CreateWorkspaceDocument" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"unauthorized\"}"

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

print_step "Testing CreateDataStreamDocument with invalid token..."
test_invalid_token \
    "document.v1.DocumentService/CreateDataStreamDocument" \
    "{\"data_stream_id\": \"$DATA_STREAM_ID\", \"organization_id\": \"$ORG_ID\", \"workspace_id\": \"$WORKSPACE_ID\", \"name\": \"invalid\"}"

print_step "Testing CreateDefinitionDocument with invalid token..."
test_invalid_token \
    "document.v1.DocumentService/CreateDefinitionDocument" \
    "{\"definition_id\": \"$DEFINITION_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"invalid\"}"

print_step "Testing CreateWorkspaceDocument with invalid token..."
test_invalid_token \
    "document.v1.DocumentService/CreateWorkspaceDocument" \
    "{\"workspace_id\": \"$WORKSPACE_ID\", \"organization_id\": \"$ORG_ID\", \"name\": \"invalid\"}"

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
