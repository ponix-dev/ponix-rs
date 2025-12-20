-- +goose Up
CREATE TABLE IF NOT EXISTS casbin_rule (
    id SERIAL PRIMARY KEY,
    ptype VARCHAR(12) NOT NULL DEFAULT '',
    v0 VARCHAR(128) NOT NULL DEFAULT '',
    v1 VARCHAR(128) NOT NULL DEFAULT '',
    v2 VARCHAR(128) NOT NULL DEFAULT '',
    v3 VARCHAR(128) NOT NULL DEFAULT '',
    v4 VARCHAR(128) NOT NULL DEFAULT '',
    v5 VARCHAR(128) NOT NULL DEFAULT '',
    CONSTRAINT unique_key_casbin_rule UNIQUE (ptype, v0, v1, v2, v3, v4, v5)
);

CREATE INDEX idx_casbin_rule_ptype ON casbin_rule(ptype);
CREATE INDEX idx_casbin_rule_v0 ON casbin_rule(v0);
CREATE INDEX idx_casbin_rule_v1 ON casbin_rule(v1);

-- +goose Down
DROP TABLE IF EXISTS casbin_rule;
