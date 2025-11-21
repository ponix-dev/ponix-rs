-- +goose Up
-- Add payload_conversion column to devices table
ALTER TABLE devices
ADD COLUMN payload_conversion TEXT NOT NULL DEFAULT '';

-- Remove the default after adding the column
-- This allows the migration to work with existing rows if any
ALTER TABLE devices
ALTER COLUMN payload_conversion DROP DEFAULT;

-- +goose Down
ALTER TABLE devices
DROP COLUMN payload_conversion;
