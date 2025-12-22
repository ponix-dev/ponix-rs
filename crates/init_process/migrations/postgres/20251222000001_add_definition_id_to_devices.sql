-- +goose Up
-- +goose StatementBegin
-- Add definition_id column (nullable initially for migration of existing data)
ALTER TABLE devices
ADD COLUMN definition_id TEXT;

-- Add foreign key constraint with RESTRICT (prevents deletion if referenced)
ALTER TABLE devices
ADD CONSTRAINT fk_devices_definition
FOREIGN KEY (definition_id)
REFERENCES end_device_definitions(id)
ON DELETE RESTRICT;

-- Index for querying devices by definition
CREATE INDEX idx_devices_definition_id ON devices(definition_id);

-- Remove payload_conversion from devices (it's now on definitions)
ALTER TABLE devices
DROP COLUMN IF EXISTS payload_conversion;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
-- Re-add payload_conversion column
ALTER TABLE devices
ADD COLUMN payload_conversion TEXT NOT NULL DEFAULT '';

ALTER TABLE devices
ALTER COLUMN payload_conversion DROP DEFAULT;

DROP INDEX IF EXISTS idx_devices_definition_id;
ALTER TABLE devices DROP CONSTRAINT IF EXISTS fk_devices_definition;
ALTER TABLE devices DROP COLUMN IF EXISTS definition_id;
-- +goose StatementEnd
