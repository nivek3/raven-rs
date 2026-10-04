-- Canonical chain metadata; applications own entity tables.
CREATE TABLE IF NOT EXISTS blocks (
    number BIGINT NOT NULL,
    hash JSONB PRIMARY KEY,
    parent_hash JSONB NOT NULL,
    status SMALLINT NOT NULL -- 1: canonical, 0: orphaned
);
CREATE UNIQUE INDEX IF NOT EXISTS blocks_canonical_height ON blocks (number) WHERE status = 1;

CREATE TABLE IF NOT EXISTS networks (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE,
    chain_id NUMERIC(20,0) NOT NULL,
    network_name TEXT NOT NULL,
    start_block BIGINT NOT NULL,
    latest_block_number BIGINT,
    latest_block_hash JSONB REFERENCES blocks(hash),
    start_parent_number BIGINT,
    start_parent_hash JSONB,
    start_parent_parent_hash JSONB
);
