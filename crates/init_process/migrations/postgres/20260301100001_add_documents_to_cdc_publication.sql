-- +goose Up
-- +goose StatementBegin
ALTER PUBLICATION ponix_cdc_publication ADD TABLE documents;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
ALTER PUBLICATION ponix_cdc_publication DROP TABLE documents;
-- +goose StatementEnd
