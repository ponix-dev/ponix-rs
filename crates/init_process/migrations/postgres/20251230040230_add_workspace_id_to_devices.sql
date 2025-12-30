-- +goose Up
-- +goose StatementBegin
ALTER TABLE devices ADD COLUMN workspace_id TEXT REFERENCES workspaces(id) ON DELETE RESTRICT;
-- +goose StatementEnd

-- +goose StatementBegin
CREATE INDEX idx_devices_workspace_id ON devices(workspace_id);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP INDEX IF EXISTS idx_devices_workspace_id;
-- +goose StatementEnd

-- +goose StatementBegin
ALTER TABLE devices DROP COLUMN workspace_id;
-- +goose StatementEnd
