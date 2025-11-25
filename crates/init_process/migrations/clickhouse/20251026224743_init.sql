-- +goose Up
-- +goose StatementBegin
CREATE TABLE IF NOT EXISTS processed_envelopes (
  end_device_id String NOT NULL,
  occurred_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data JSON NOT NULL
) ENGINE = MergeTree()
PRIMARY KEY (occurred_at, end_device_id)
ORDER BY (occurred_at, end_device_id)
PARTITION BY toYYYYMM(occurred_at)
SETTINGS index_granularity = 8192;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP TABLE IF EXISTS processed_envelopes;
-- +goose StatementEnd
