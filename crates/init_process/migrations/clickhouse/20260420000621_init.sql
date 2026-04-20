-- +goose Up
-- +goose StatementBegin
SET allow_experimental_json_type = 1
-- +goose StatementEnd

-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS processed_envelopes (
    end_device_id String NOT NULL,
    organization_id String NOT NULL,
    received_at DateTime NOT NULL,
    processed_at DateTime NOT NULL,
    data JSON NOT NULL
) ENGINE = MergeTree()
PRIMARY KEY (organization_id, received_at, end_device_id)
ORDER BY (organization_id, received_at, end_device_id)
PARTITION BY toYYYYMM(received_at)
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP TABLE IF EXISTS processed_envelopes
-- +goose StatementEnd
