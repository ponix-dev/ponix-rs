-- +goose Up

-- document_data_streams: links documents to data streams (scoped to workspace + org)
CREATE TABLE document_data_streams (
    document_id TEXT NOT NULL REFERENCES documents(document_id) ON DELETE CASCADE,
    data_stream_id TEXT NOT NULL REFERENCES data_streams(data_stream_id) ON DELETE CASCADE,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (document_id, data_stream_id)
);
CREATE INDEX idx_doc_ds_data_stream ON document_data_streams(data_stream_id);

-- document_definitions: links documents to data stream definitions
CREATE TABLE document_definitions (
    document_id TEXT NOT NULL REFERENCES documents(document_id) ON DELETE CASCADE,
    definition_id TEXT NOT NULL REFERENCES data_stream_definitions(id) ON DELETE CASCADE,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (document_id, definition_id)
);
CREATE INDEX idx_doc_def_definition ON document_definitions(definition_id);

-- document_workspaces: links documents to workspaces
CREATE TABLE document_workspaces (
    document_id TEXT NOT NULL REFERENCES documents(document_id) ON DELETE CASCADE,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (document_id, workspace_id)
);
CREATE INDEX idx_doc_ws_workspace ON document_workspaces(workspace_id);

-- +goose Down
DROP TABLE IF EXISTS document_workspaces;
DROP TABLE IF EXISTS document_definitions;
DROP TABLE IF EXISTS document_data_streams;
