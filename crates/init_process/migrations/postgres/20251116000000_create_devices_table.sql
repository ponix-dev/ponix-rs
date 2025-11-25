-- +goose Up
-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    device_name TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Index for querying devices by organization
CREATE INDEX idx_devices_organization_id ON devices(organization_id);

-- Index for sorting by creation time
CREATE INDEX idx_devices_created_at ON devices(created_at DESC);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP INDEX IF EXISTS idx_devices_created_at;
DROP INDEX IF EXISTS idx_devices_organization_id;
DROP TABLE IF EXISTS devices;
-- +goose StatementEnd
