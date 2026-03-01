-- +goose Up
LOAD 'age';
SET search_path = ag_catalog, "$user", public;
SELECT create_vlabel('ponix_graph', 'Document');
SELECT create_vlabel('ponix_graph', 'DataStream');
SELECT create_vlabel('ponix_graph', 'DataStreamDefinition');
SELECT create_vlabel('ponix_graph', 'Workspace');
SELECT create_elabel('ponix_graph', 'HAS_DOCUMENT');
SET search_path = "$user", public;

-- +goose Down
LOAD 'age';
SET search_path = ag_catalog, "$user", public;
SELECT drop_elabel('ponix_graph', 'HAS_DOCUMENT');
SELECT drop_vlabel('ponix_graph', 'Workspace');
SELECT drop_vlabel('ponix_graph', 'DataStreamDefinition');
SELECT drop_vlabel('ponix_graph', 'DataStream');
SELECT drop_vlabel('ponix_graph', 'Document');
SET search_path = "$user", public;
