-- +goose Up
-- Add name column to gateways table
ALTER TABLE gateways ADD COLUMN name TEXT NOT NULL DEFAULT '';

-- +goose Down
ALTER TABLE gateways DROP COLUMN name;
