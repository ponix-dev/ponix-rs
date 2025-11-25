-- +goose Up
-- +goose StatementBegin
ALTER TABLE gateways
ADD CONSTRAINT fk_gateways_organization
FOREIGN KEY (organization_id)
REFERENCES organizations(id);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
ALTER TABLE gateways
DROP CONSTRAINT IF EXISTS fk_gateways_organization;
-- +goose StatementEnd
