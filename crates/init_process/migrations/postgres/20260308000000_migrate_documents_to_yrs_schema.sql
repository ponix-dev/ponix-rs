-- +goose Up
-- Migrate documents from file-based storage to Yrs CRDT collaborative schema

-- Drop file-storage columns and index
DROP INDEX IF EXISTS idx_documents_object_store_key;
ALTER TABLE documents DROP COLUMN IF EXISTS mime_type;
ALTER TABLE documents DROP COLUMN IF EXISTS size_bytes;
ALTER TABLE documents DROP COLUMN IF EXISTS object_store_key;
ALTER TABLE documents DROP COLUMN IF EXISTS checksum;

-- Add Yrs CRDT columns
ALTER TABLE documents ADD COLUMN yrs_state BYTEA NOT NULL DEFAULT '\x'::bytea;
ALTER TABLE documents ADD COLUMN yrs_state_vector BYTEA NOT NULL DEFAULT '\x'::bytea;
ALTER TABLE documents ADD COLUMN content_text TEXT NOT NULL DEFAULT '';
ALTER TABLE documents ADD COLUMN content_html TEXT NOT NULL DEFAULT '';

-- +goose Down
-- Reverse: remove Yrs columns, restore file-storage columns
ALTER TABLE documents DROP COLUMN IF EXISTS yrs_state;
ALTER TABLE documents DROP COLUMN IF EXISTS yrs_state_vector;
ALTER TABLE documents DROP COLUMN IF EXISTS content_text;
ALTER TABLE documents DROP COLUMN IF EXISTS content_html;

ALTER TABLE documents ADD COLUMN mime_type TEXT NOT NULL DEFAULT '';
ALTER TABLE documents ADD COLUMN size_bytes BIGINT NOT NULL DEFAULT 0;
ALTER TABLE documents ADD COLUMN object_store_key TEXT NOT NULL DEFAULT '';
ALTER TABLE documents ADD COLUMN checksum TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_documents_object_store_key ON documents (object_store_key);
