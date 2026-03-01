-- +goose Up
DROP INDEX IF EXISTS idx_documents_status;
ALTER TABLE documents DROP COLUMN IF EXISTS status;

-- +goose Down
ALTER TABLE documents ADD COLUMN status VARCHAR(20) NOT NULL DEFAULT 'pending';
CREATE INDEX idx_documents_status ON documents(status);
