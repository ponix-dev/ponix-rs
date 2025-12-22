-- +goose Up
-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS end_device_definitions (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    name TEXT NOT NULL,
    json_schema TEXT NOT NULL DEFAULT '{}',
    payload_conversion TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_end_device_definitions_organization
        FOREIGN KEY (organization_id)
        REFERENCES organizations(id)
);

-- Index for querying definitions by organization
CREATE INDEX idx_end_device_definitions_organization_id ON end_device_definitions(organization_id);

-- Index for sorting by creation time
CREATE INDEX idx_end_device_definitions_created_at ON end_device_definitions(created_at DESC);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP INDEX IF EXISTS idx_end_device_definitions_created_at;
DROP INDEX IF EXISTS idx_end_device_definitions_organization_id;
DROP TABLE IF EXISTS end_device_definitions;
-- +goose StatementEnd
