-- +goose Up
-- +goose StatementBegin
ALTER PUBLICATION ponix_cdc_publication ADD TABLE workspaces;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
ALTER PUBLICATION ponix_cdc_publication DROP TABLE workspaces;
-- +goose StatementEnd
