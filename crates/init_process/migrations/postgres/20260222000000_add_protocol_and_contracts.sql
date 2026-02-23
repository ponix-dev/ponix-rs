-- +goose Up
-- +goose StatementBegin

-- Add contracts column as JSONB with default empty array
ALTER TABLE end_device_definitions ADD COLUMN contracts JSONB NOT NULL DEFAULT '[]';

-- Migrate existing data: wrap payload_conversion + json_schema into a single-element contracts array
UPDATE end_device_definitions
SET contracts = jsonb_build_array(
    jsonb_build_object(
        'match_expression', 'true',
        'transform_expression', payload_conversion,
        'json_schema', json_schema
    )
)
WHERE payload_conversion IS NOT NULL AND payload_conversion != '';

-- Enforce at least one contract at the database level
ALTER TABLE end_device_definitions ADD CONSTRAINT contracts_non_empty CHECK (jsonb_array_length(contracts) > 0);

-- Drop old columns
ALTER TABLE end_device_definitions DROP COLUMN json_schema;
ALTER TABLE end_device_definitions DROP COLUMN payload_conversion;

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

-- Drop the non-empty constraint before reverting schema
ALTER TABLE end_device_definitions DROP CONSTRAINT IF EXISTS contracts_non_empty;

-- Re-add old columns
ALTER TABLE end_device_definitions ADD COLUMN json_schema TEXT NOT NULL DEFAULT '{}';
ALTER TABLE end_device_definitions ADD COLUMN payload_conversion TEXT NOT NULL DEFAULT '';

-- Extract first contract back to flat fields
UPDATE end_device_definitions
SET json_schema = COALESCE(contracts->0->>'json_schema', '{}'),
    payload_conversion = COALESCE(contracts->0->>'transform_expression', '')
WHERE jsonb_array_length(contracts) > 0;

-- Drop new columns
ALTER TABLE end_device_definitions DROP COLUMN contracts;

-- +goose StatementEnd
