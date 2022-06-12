/**************************************************************
* CREATE TABLES
**************************************************************/

CREATE TABLE ethereum_blocks (
  id SERIAL PRIMARY KEY,
  hash VARCHAR NOT NULL,
  parent_hash VARCHAR,
  number BIGINT NOT NULL,
  UNIQUE (hash)
)