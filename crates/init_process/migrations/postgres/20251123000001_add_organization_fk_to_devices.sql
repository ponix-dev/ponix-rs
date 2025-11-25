-- +goose Up
-- +goose StatementBegin
ALTER TABLE devices
ADD CONSTRAINT fk_devices_organization
FOREIGN KEY (organization_id)
REFERENCES organizations(id);

CREATE INDEX idx_devices_organization_id_fk ON devices(organization_id);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP INDEX IF EXISTS idx_devices_organization_id_fk;
ALTER TABLE devices
DROP CONSTRAINT IF EXISTS fk_devices_organization;
-- +goose StatementEnd
