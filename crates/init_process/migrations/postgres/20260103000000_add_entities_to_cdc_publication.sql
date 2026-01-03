-- +goose Up
-- +goose StatementBegin
ALTER PUBLICATION ponix_cdc_publication ADD TABLE devices;
ALTER PUBLICATION ponix_cdc_publication ADD TABLE organizations;
ALTER PUBLICATION ponix_cdc_publication ADD TABLE users;
ALTER PUBLICATION ponix_cdc_publication ADD TABLE end_device_definitions;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
ALTER PUBLICATION ponix_cdc_publication DROP TABLE devices;
ALTER PUBLICATION ponix_cdc_publication DROP TABLE organizations;
ALTER PUBLICATION ponix_cdc_publication DROP TABLE users;
ALTER PUBLICATION ponix_cdc_publication DROP TABLE end_device_definitions;
-- +goose StatementEnd
