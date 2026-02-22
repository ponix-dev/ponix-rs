-- +goose Up
-- +goose StatementBegin
-- Create new table with received_at replacing occurred_at
CREATE TABLE IF NOT EXISTS processed_envelopes_new (
  organization_id String NOT NULL,
  end_device_id String NOT NULL,
  received_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data JSON NOT NULL
) ENGINE = MergeTree()
PRIMARY KEY (organization_id, received_at, end_device_id)
ORDER BY (organization_id, received_at, end_device_id)
PARTITION BY toYYYYMM(received_at)
SETTINGS index_granularity = 8192;
-- +goose StatementEnd

-- +goose StatementBegin
-- Migrate data from old table
INSERT INTO processed_envelopes_new
SELECT
  organization_id,
  end_device_id,
  occurred_at AS received_at,
  processed_at,
  data
FROM processed_envelopes;
-- +goose StatementEnd

-- +goose StatementBegin
-- Drop old table
DROP TABLE IF EXISTS processed_envelopes;
-- +goose StatementEnd

-- +goose StatementBegin
-- Rename new table to original name
RENAME TABLE processed_envelopes_new TO processed_envelopes;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
-- Create old table structure with occurred_at
CREATE TABLE IF NOT EXISTS processed_envelopes_old (
  organization_id String NOT NULL,
  end_device_id String NOT NULL,
  occurred_at DateTime NOT NULL,
  processed_at DateTime NOT NULL,
  data JSON NOT NULL
) ENGINE = MergeTree()
PRIMARY KEY (organization_id, occurred_at, end_device_id)
ORDER BY (organization_id, occurred_at, end_device_id)
PARTITION BY toYYYYMM(occurred_at)
SETTINGS index_granularity = 8192;
-- +goose StatementEnd

-- +goose StatementBegin
-- Migrate data back
INSERT INTO processed_envelopes_old
SELECT
  organization_id,
  end_device_id,
  received_at AS occurred_at,
  processed_at,
  data
FROM processed_envelopes;
-- +goose StatementEnd

-- +goose StatementBegin
DROP TABLE IF EXISTS processed_envelopes;
-- +goose StatementEnd

-- +goose StatementBegin
RENAME TABLE processed_envelopes_old TO processed_envelopes;
-- +goose StatementEnd
