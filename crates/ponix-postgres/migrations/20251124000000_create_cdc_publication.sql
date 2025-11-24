-- +goose Up
-- +goose StatementBegin

-- Create publication for gateway CDC
-- Note: This requires PostgreSQL logical replication to be enabled
-- The following postgresql.conf settings are required:
-- wal_level = logical
-- max_replication_slots = 4
-- max_wal_senders = 4

-- Create publication for gateways table
-- Using IF NOT EXISTS to make migration idempotent
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_publication WHERE pubname = 'ponix_cdc_publication'
    ) THEN
        CREATE PUBLICATION ponix_cdc_publication FOR TABLE gateways;
        RAISE NOTICE 'Created publication ponix_cdc_publication for table gateways';
    ELSE
        RAISE NOTICE 'Publication ponix_cdc_publication already exists';
    END IF;
END $$;

-- Note: The replication slot will be created automatically by the ETL library
-- when it connects for the first time. The slot name is configured via
-- environment variable PONIX_CDC_SLOT (defaults to 'ponix_cdc_slot')

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

-- Drop publication if exists
DROP PUBLICATION IF EXISTS ponix_cdc_publication;

-- Note: Replication slots must be dropped manually if they exist.
-- You can check for slots with:
-- SELECT * FROM pg_replication_slots WHERE slot_name = 'ponix_cdc_slot';
--
-- To drop a slot manually (when the CDC process is not running):
-- SELECT pg_drop_replication_slot('ponix_cdc_slot');

-- +goose StatementEnd
