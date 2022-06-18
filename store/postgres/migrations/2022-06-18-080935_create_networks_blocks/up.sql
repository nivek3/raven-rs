-- Your SQL goes here

-- Stores list of Ethereum networks (e.g. mainnet, ropsten) and optional pointer to head block
CREATE TABLE IF NOT EXISTS ethereum_networks (
    name VARCHAR PRIMARY KEY,
    head_block_hash VARCHAR,
    head_block_number BIGINT,
    CHECK ((head_block_hash IS NULL) = (head_block_number IS NULL))
);

CREATE TABLE IF NOT EXISTS ethereum_blocks (
    hash VARCHAR PRIMARY KEY,
    number BIGINT NOT NULL,
    parent_hash VARCHAR NOT NULL,
    network_name VARCHAR NOT NULL REFERENCES ethereum_networks (name),
    data JSONB NOT NULL
);

CREATE INDEX ethereum_blocks_name_number ON ethereum_blocks(network_name, number);