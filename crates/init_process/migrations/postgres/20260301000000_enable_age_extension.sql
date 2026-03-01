-- +goose Up
CREATE EXTENSION IF NOT EXISTS age;
LOAD 'age';
SET search_path = ag_catalog, "$user", public;
SELECT create_graph('ponix_graph');
SET search_path = "$user", public;

-- +goose Down
SELECT drop_graph('ponix_graph', true);
DROP EXTENSION IF EXISTS age CASCADE;
