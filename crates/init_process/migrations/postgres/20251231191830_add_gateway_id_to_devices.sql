-- +goose Up
-- +goose StatementBegin
ALTER TABLE devices ADD COLUMN gateway_id TEXT NOT NULL REFERENCES gateways(gateway_id) ON DELETE RESTRICT;
-- +goose StatementEnd

-- +goose StatementBegin
CREATE INDEX idx_devices_gateway_id ON devices(gateway_id);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP INDEX IF EXISTS idx_devices_gateway_id;
-- +goose StatementEnd

-- +goose StatementBegin
ALTER TABLE devices DROP COLUMN gateway_id;
-- +goose StatementEnd
