-- +goose Up
-- +goose StatementBegin
ALTER TABLE processed_envelopes RENAME COLUMN occurred_at TO received_at;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
ALTER TABLE processed_envelopes RENAME COLUMN received_at TO occurred_at;
-- +goose StatementEnd
