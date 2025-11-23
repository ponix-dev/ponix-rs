-- +goose Up
-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS gateways (
    gateway_id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    gateway_type TEXT NOT NULL,
    gateway_config JSONB NOT NULL,
    deleted_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Index for querying gateways by organization
CREATE INDEX idx_gateways_organization_id ON gateways(organization_id);

-- Index for filtering by gateway type
CREATE INDEX idx_gateways_gateway_type ON gateways(gateway_type);

-- Index for soft delete filtering
CREATE INDEX idx_gateways_deleted_at ON gateways(deleted_at);

-- Index for sorting by creation time
CREATE INDEX idx_gateways_created_at ON gateways(created_at DESC);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP INDEX IF EXISTS idx_gateways_created_at;
DROP INDEX IF EXISTS idx_gateways_deleted_at;
DROP INDEX IF EXISTS idx_gateways_gateway_type;
DROP INDEX IF EXISTS idx_gateways_organization_id;
DROP TABLE IF EXISTS gateways;
-- +goose StatementEnd
