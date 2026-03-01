-- +goose Up
-- +goose StatementBegin
ALTER TABLE devices RENAME TO data_streams;
ALTER TABLE data_streams RENAME COLUMN device_id TO data_stream_id;
ALTER TABLE data_streams RENAME COLUMN device_name TO name;
ALTER TABLE end_device_definitions RENAME TO data_stream_definitions;

ALTER PUBLICATION ponix_cdc_publication DROP TABLE data_streams;
ALTER PUBLICATION ponix_cdc_publication DROP TABLE data_stream_definitions;
ALTER PUBLICATION ponix_cdc_publication ADD TABLE data_streams;
ALTER PUBLICATION ponix_cdc_publication ADD TABLE data_stream_definitions;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
ALTER TABLE data_streams RENAME TO devices;
ALTER TABLE devices RENAME COLUMN data_stream_id TO device_id;
ALTER TABLE devices RENAME COLUMN name TO device_name;
ALTER TABLE data_stream_definitions RENAME TO end_device_definitions;

ALTER PUBLICATION ponix_cdc_publication DROP TABLE devices;
ALTER PUBLICATION ponix_cdc_publication DROP TABLE end_device_definitions;
ALTER PUBLICATION ponix_cdc_publication ADD TABLE devices;
ALTER PUBLICATION ponix_cdc_publication ADD TABLE end_device_definitions;
-- +goose StatementEnd
