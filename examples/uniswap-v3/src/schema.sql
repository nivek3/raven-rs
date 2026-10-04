-- Application-owned native entities for the selected Uniswap V3 deployment.

CREATE TABLE IF NOT EXISTS "factory" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "pool_count" NUMERIC NOT NULL,
    "tx_count" NUMERIC NOT NULL,
    "total_volume_usd" NUMERIC NOT NULL,
    "total_volume_eth" NUMERIC NOT NULL,
    "total_fees_usd" NUMERIC NOT NULL,
    "total_fees_eth" NUMERIC NOT NULL,
    "untracked_volume_usd" NUMERIC NOT NULL,
    "total_value_locked_usd" NUMERIC NOT NULL,
    "total_value_locked_eth" NUMERIC NOT NULL,
    "total_value_locked_usd_untracked" NUMERIC NOT NULL,
    "total_value_locked_eth_untracked" NUMERIC NOT NULL,
    "owner" TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS factory_current ON "factory" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS factory_history ON "factory" USING gist(block_range);

CREATE TABLE IF NOT EXISTS "bundle" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "eth_price_usd" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS bundle_current ON "bundle" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS bundle_history ON "bundle" USING gist(block_range);

CREATE TABLE IF NOT EXISTS "token" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "decimals" NUMERIC NOT NULL,
    "total_supply" NUMERIC NOT NULL,
    "volume" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "untracked_volume_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "tx_count" NUMERIC NOT NULL,
    "pool_count" NUMERIC NOT NULL,
    "total_value_locked" NUMERIC NOT NULL,
    "total_value_locked_usd" NUMERIC NOT NULL,
    "total_value_locked_usd_untracked" NUMERIC NOT NULL,
    "derived_eth" NUMERIC NOT NULL,
    "whitelist_pools" TEXT[] NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS token_current ON "token" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS token_history ON "token" USING gist(block_range);

CREATE TABLE IF NOT EXISTS "pool" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "created_at_timestamp" NUMERIC NOT NULL,
    "created_at_block_number" BIGINT NOT NULL,
    "token0" TEXT NOT NULL,
    "token1" TEXT NOT NULL,
    "fee_tier" NUMERIC NOT NULL,
    "liquidity" NUMERIC NOT NULL,
    "sqrt_price" NUMERIC NOT NULL,
    "fee_growth_global0_x128" NUMERIC NOT NULL,
    "fee_growth_global1_x128" NUMERIC NOT NULL,
    "token0_price" NUMERIC NOT NULL,
    "token1_price" NUMERIC NOT NULL,
    "tick" NUMERIC,
    "observation_index" NUMERIC NOT NULL,
    "volume_token0" NUMERIC NOT NULL,
    "volume_token1" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "untracked_volume_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "tx_count" NUMERIC NOT NULL,
    "collected_fees_token0" NUMERIC NOT NULL,
    "collected_fees_token1" NUMERIC NOT NULL,
    "collected_fees_usd" NUMERIC NOT NULL,
    "total_value_locked_token0" NUMERIC NOT NULL,
    "total_value_locked_token1" NUMERIC NOT NULL,
    "total_value_locked_eth" NUMERIC NOT NULL,
    "total_value_locked_usd" NUMERIC NOT NULL,
    "total_value_locked_usd_untracked" NUMERIC NOT NULL,
    "liquidity_provider_count" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS pool_current ON "pool" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS pool_history ON "pool" USING gist(block_range);

CREATE INDEX IF NOT EXISTS pool_token0 ON "pool" ("token0");

CREATE INDEX IF NOT EXISTS pool_token1 ON "pool" ("token1");

CREATE TABLE IF NOT EXISTS "tick" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "pool_address" TEXT,
    "tick_idx" NUMERIC NOT NULL,
    "pool" TEXT NOT NULL,
    "liquidity_gross" NUMERIC NOT NULL,
    "liquidity_net" NUMERIC NOT NULL,
    "price0" NUMERIC NOT NULL,
    "price1" NUMERIC NOT NULL,
    "volume_token0" NUMERIC NOT NULL,
    "volume_token1" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "untracked_volume_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "collected_fees_token0" NUMERIC NOT NULL,
    "collected_fees_token1" NUMERIC NOT NULL,
    "collected_fees_usd" NUMERIC NOT NULL,
    "created_at_timestamp" NUMERIC NOT NULL,
    "created_at_block_number" BIGINT NOT NULL,
    "liquidity_provider_count" NUMERIC NOT NULL,
    "fee_growth_outside0_x128" NUMERIC NOT NULL,
    "fee_growth_outside1_x128" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS tick_current ON "tick" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS tick_history ON "tick" USING gist(block_range);

CREATE INDEX IF NOT EXISTS tick_pool ON "tick" ("pool");

CREATE TABLE IF NOT EXISTS "position" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "owner" TEXT NOT NULL,
    "pool" TEXT NOT NULL,
    "token0" TEXT NOT NULL,
    "token1" TEXT NOT NULL,
    "tick_lower" TEXT NOT NULL,
    "tick_upper" TEXT NOT NULL,
    "liquidity" NUMERIC NOT NULL,
    "deposited_token0" NUMERIC NOT NULL,
    "deposited_token1" NUMERIC NOT NULL,
    "withdrawn_token0" NUMERIC NOT NULL,
    "withdrawn_token1" NUMERIC NOT NULL,
    "collected_fees_token0" NUMERIC NOT NULL,
    "collected_fees_token1" NUMERIC NOT NULL,
    "transaction" TEXT NOT NULL,
    "fee_growth_inside0_last_x128" NUMERIC NOT NULL,
    "fee_growth_inside1_last_x128" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS position_current ON "position" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS position_history ON "position" USING gist(block_range);

CREATE INDEX IF NOT EXISTS position_pool ON "position" ("pool");

CREATE INDEX IF NOT EXISTS position_token0 ON "position" ("token0");

CREATE INDEX IF NOT EXISTS position_token1 ON "position" ("token1");

CREATE INDEX IF NOT EXISTS position_tick_lower ON "position" ("tick_lower");

CREATE INDEX IF NOT EXISTS position_tick_upper ON "position" ("tick_upper");

CREATE INDEX IF NOT EXISTS position_transaction ON "position" ("transaction");

CREATE TABLE IF NOT EXISTS "position_snapshot" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "owner" TEXT NOT NULL,
    "pool" TEXT NOT NULL,
    "position" TEXT NOT NULL,
    "block_number" BIGINT NOT NULL,
    "timestamp" NUMERIC NOT NULL,
    "liquidity" NUMERIC NOT NULL,
    "deposited_token0" NUMERIC NOT NULL,
    "deposited_token1" NUMERIC NOT NULL,
    "withdrawn_token0" NUMERIC NOT NULL,
    "withdrawn_token1" NUMERIC NOT NULL,
    "collected_fees_token0" NUMERIC NOT NULL,
    "collected_fees_token1" NUMERIC NOT NULL,
    "transaction" TEXT NOT NULL,
    "fee_growth_inside0_last_x128" NUMERIC NOT NULL,
    "fee_growth_inside1_last_x128" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS position_snapshot_current ON "position_snapshot" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS position_snapshot_history ON "position_snapshot" USING gist(block_range);

CREATE INDEX IF NOT EXISTS position_snapshot_pool ON "position_snapshot" ("pool");

CREATE INDEX IF NOT EXISTS position_snapshot_position ON "position_snapshot" ("position");

CREATE INDEX IF NOT EXISTS position_snapshot_transaction ON "position_snapshot" ("transaction");

CREATE TABLE IF NOT EXISTS "transaction" (
    vid BIGSERIAL PRIMARY KEY,
    "id" TEXT NOT NULL,
    "block_number" BIGINT NOT NULL,
    "timestamp" NUMERIC NOT NULL,
    "gas_used" NUMERIC NOT NULL,
    "gas_price" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS transaction_current ON "transaction" (id);

CREATE INDEX IF NOT EXISTS transaction_block ON "transaction" (block_number);

CREATE TABLE IF NOT EXISTS "mint" (
    vid BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    "id" TEXT NOT NULL,
    "transaction" TEXT NOT NULL,
    "timestamp" NUMERIC NOT NULL,
    "pool" TEXT NOT NULL,
    "token0" TEXT NOT NULL,
    "token1" TEXT NOT NULL,
    "owner" TEXT NOT NULL,
    "sender" TEXT,
    "origin" TEXT NOT NULL,
    "amount" NUMERIC NOT NULL,
    "amount0" NUMERIC NOT NULL,
    "amount1" NUMERIC NOT NULL,
    "amount_usd" NUMERIC,
    "tick_lower" NUMERIC NOT NULL,
    "tick_upper" NUMERIC NOT NULL,
    "log_index" NUMERIC
);

CREATE UNIQUE INDEX IF NOT EXISTS mint_current ON "mint" (id);

CREATE INDEX IF NOT EXISTS mint_block ON "mint" (block_number);

CREATE INDEX IF NOT EXISTS mint_transaction ON "mint" ("transaction");

CREATE INDEX IF NOT EXISTS mint_pool ON "mint" ("pool");

CREATE INDEX IF NOT EXISTS mint_token0 ON "mint" ("token0");

CREATE INDEX IF NOT EXISTS mint_token1 ON "mint" ("token1");

CREATE TABLE IF NOT EXISTS "burn" (
    vid BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    "id" TEXT NOT NULL,
    "transaction" TEXT NOT NULL,
    "pool" TEXT NOT NULL,
    "token0" TEXT NOT NULL,
    "token1" TEXT NOT NULL,
    "timestamp" NUMERIC NOT NULL,
    "owner" TEXT,
    "origin" TEXT NOT NULL,
    "amount" NUMERIC NOT NULL,
    "amount0" NUMERIC NOT NULL,
    "amount1" NUMERIC NOT NULL,
    "amount_usd" NUMERIC,
    "tick_lower" NUMERIC NOT NULL,
    "tick_upper" NUMERIC NOT NULL,
    "log_index" NUMERIC
);

CREATE UNIQUE INDEX IF NOT EXISTS burn_current ON "burn" (id);

CREATE INDEX IF NOT EXISTS burn_block ON "burn" (block_number);

CREATE INDEX IF NOT EXISTS burn_transaction ON "burn" ("transaction");

CREATE INDEX IF NOT EXISTS burn_pool ON "burn" ("pool");

CREATE INDEX IF NOT EXISTS burn_token0 ON "burn" ("token0");

CREATE INDEX IF NOT EXISTS burn_token1 ON "burn" ("token1");

CREATE TABLE IF NOT EXISTS "swap" (
    vid BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    "id" TEXT NOT NULL,
    "transaction" TEXT NOT NULL,
    "timestamp" NUMERIC NOT NULL,
    "pool" TEXT NOT NULL,
    "token0" TEXT NOT NULL,
    "token1" TEXT NOT NULL,
    "sender" TEXT NOT NULL,
    "recipient" TEXT NOT NULL,
    "origin" TEXT NOT NULL,
    "amount0" NUMERIC NOT NULL,
    "amount1" NUMERIC NOT NULL,
    "amount_usd" NUMERIC NOT NULL,
    "sqrt_price_x96" NUMERIC NOT NULL,
    "tick" NUMERIC NOT NULL,
    "log_index" NUMERIC
);

CREATE UNIQUE INDEX IF NOT EXISTS swap_current ON "swap" (id);

CREATE INDEX IF NOT EXISTS swap_block ON "swap" (block_number);

CREATE INDEX IF NOT EXISTS swap_transaction ON "swap" ("transaction");

CREATE INDEX IF NOT EXISTS swap_pool ON "swap" ("pool");

CREATE INDEX IF NOT EXISTS swap_token0 ON "swap" ("token0");

CREATE INDEX IF NOT EXISTS swap_token1 ON "swap" ("token1");

CREATE TABLE IF NOT EXISTS "collect" (
    vid BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    "id" TEXT NOT NULL,
    "transaction" TEXT NOT NULL,
    "timestamp" NUMERIC NOT NULL,
    "pool" TEXT NOT NULL,
    "owner" TEXT,
    "amount0" NUMERIC NOT NULL,
    "amount1" NUMERIC NOT NULL,
    "amount_usd" NUMERIC,
    "tick_lower" NUMERIC NOT NULL,
    "tick_upper" NUMERIC NOT NULL,
    "log_index" NUMERIC
);

CREATE UNIQUE INDEX IF NOT EXISTS collect_current ON "collect" (id);

CREATE INDEX IF NOT EXISTS collect_block ON "collect" (block_number);

CREATE INDEX IF NOT EXISTS collect_transaction ON "collect" ("transaction");

CREATE INDEX IF NOT EXISTS collect_pool ON "collect" ("pool");

CREATE TABLE IF NOT EXISTS "flash" (
    vid BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    "id" TEXT NOT NULL,
    "transaction" TEXT NOT NULL,
    "timestamp" NUMERIC NOT NULL,
    "pool" TEXT NOT NULL,
    "sender" TEXT NOT NULL,
    "recipient" TEXT NOT NULL,
    "amount0" NUMERIC NOT NULL,
    "amount1" NUMERIC NOT NULL,
    "amount_usd" NUMERIC NOT NULL,
    "amount0_paid" NUMERIC NOT NULL,
    "amount1_paid" NUMERIC NOT NULL,
    "log_index" NUMERIC
);

CREATE UNIQUE INDEX IF NOT EXISTS flash_current ON "flash" (id);

CREATE INDEX IF NOT EXISTS flash_block ON "flash" (block_number);

CREATE INDEX IF NOT EXISTS flash_transaction ON "flash" ("transaction");

CREATE INDEX IF NOT EXISTS flash_pool ON "flash" ("pool");

CREATE TABLE IF NOT EXISTS "uniswap_day_data" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "date" INTEGER NOT NULL,
    "volume_eth" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "volume_usd_untracked" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "tx_count" NUMERIC NOT NULL,
    "tvl_usd" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uniswap_day_data_current ON "uniswap_day_data" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS uniswap_day_data_history ON "uniswap_day_data" USING gist(block_range);

CREATE TABLE IF NOT EXISTS "pool_day_data" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "date" INTEGER NOT NULL,
    "pool" TEXT NOT NULL,
    "liquidity" NUMERIC NOT NULL,
    "sqrt_price" NUMERIC NOT NULL,
    "token0_price" NUMERIC NOT NULL,
    "token1_price" NUMERIC NOT NULL,
    "tick" NUMERIC,
    "fee_growth_global0_x128" NUMERIC NOT NULL,
    "fee_growth_global1_x128" NUMERIC NOT NULL,
    "tvl_usd" NUMERIC NOT NULL,
    "volume_token0" NUMERIC NOT NULL,
    "volume_token1" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "tx_count" NUMERIC NOT NULL,
    "open" NUMERIC NOT NULL,
    "high" NUMERIC NOT NULL,
    "low" NUMERIC NOT NULL,
    "close" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS pool_day_data_current ON "pool_day_data" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS pool_day_data_history ON "pool_day_data" USING gist(block_range);

CREATE INDEX IF NOT EXISTS pool_day_data_pool ON "pool_day_data" ("pool");

CREATE TABLE IF NOT EXISTS "pool_hour_data" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "period_start_unix" INTEGER NOT NULL,
    "pool" TEXT NOT NULL,
    "liquidity" NUMERIC NOT NULL,
    "sqrt_price" NUMERIC NOT NULL,
    "token0_price" NUMERIC NOT NULL,
    "token1_price" NUMERIC NOT NULL,
    "tick" NUMERIC,
    "fee_growth_global0_x128" NUMERIC NOT NULL,
    "fee_growth_global1_x128" NUMERIC NOT NULL,
    "tvl_usd" NUMERIC NOT NULL,
    "volume_token0" NUMERIC NOT NULL,
    "volume_token1" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "tx_count" NUMERIC NOT NULL,
    "open" NUMERIC NOT NULL,
    "high" NUMERIC NOT NULL,
    "low" NUMERIC NOT NULL,
    "close" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS pool_hour_data_current ON "pool_hour_data" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS pool_hour_data_history ON "pool_hour_data" USING gist(block_range);

CREATE INDEX IF NOT EXISTS pool_hour_data_pool ON "pool_hour_data" ("pool");

CREATE TABLE IF NOT EXISTS "tick_hour_data" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "period_start_unix" INTEGER NOT NULL,
    "pool" TEXT NOT NULL,
    "tick" TEXT NOT NULL,
    "liquidity_gross" NUMERIC NOT NULL,
    "liquidity_net" NUMERIC NOT NULL,
    "volume_token0" NUMERIC NOT NULL,
    "volume_token1" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS tick_hour_data_current ON "tick_hour_data" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS tick_hour_data_history ON "tick_hour_data" USING gist(block_range);

CREATE INDEX IF NOT EXISTS tick_hour_data_pool ON "tick_hour_data" ("pool");

CREATE INDEX IF NOT EXISTS tick_hour_data_tick ON "tick_hour_data" ("tick");

CREATE TABLE IF NOT EXISTS "tick_day_data" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "date" INTEGER NOT NULL,
    "pool" TEXT NOT NULL,
    "tick" TEXT NOT NULL,
    "liquidity_gross" NUMERIC NOT NULL,
    "liquidity_net" NUMERIC NOT NULL,
    "volume_token0" NUMERIC NOT NULL,
    "volume_token1" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "fee_growth_outside0_x128" NUMERIC NOT NULL,
    "fee_growth_outside1_x128" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS tick_day_data_current ON "tick_day_data" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS tick_day_data_history ON "tick_day_data" USING gist(block_range);

CREATE INDEX IF NOT EXISTS tick_day_data_pool ON "tick_day_data" ("pool");

CREATE INDEX IF NOT EXISTS tick_day_data_tick ON "tick_day_data" ("tick");

CREATE TABLE IF NOT EXISTS "token_day_data" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "date" INTEGER NOT NULL,
    "token" TEXT NOT NULL,
    "volume" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "untracked_volume_usd" NUMERIC NOT NULL,
    "total_value_locked" NUMERIC NOT NULL,
    "total_value_locked_usd" NUMERIC NOT NULL,
    "price_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "open" NUMERIC NOT NULL,
    "high" NUMERIC NOT NULL,
    "low" NUMERIC NOT NULL,
    "close" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS token_day_data_current ON "token_day_data" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS token_day_data_history ON "token_day_data" USING gist(block_range);

CREATE INDEX IF NOT EXISTS token_day_data_token ON "token_day_data" ("token");

CREATE TABLE IF NOT EXISTS "token_hour_data" (
    vid BIGSERIAL PRIMARY KEY,
    block_range INT8RANGE NOT NULL,
    "id" TEXT NOT NULL,
    "period_start_unix" INTEGER NOT NULL,
    "token" TEXT NOT NULL,
    "volume" NUMERIC NOT NULL,
    "volume_usd" NUMERIC NOT NULL,
    "untracked_volume_usd" NUMERIC NOT NULL,
    "total_value_locked" NUMERIC NOT NULL,
    "total_value_locked_usd" NUMERIC NOT NULL,
    "price_usd" NUMERIC NOT NULL,
    "fees_usd" NUMERIC NOT NULL,
    "open" NUMERIC NOT NULL,
    "high" NUMERIC NOT NULL,
    "low" NUMERIC NOT NULL,
    "close" NUMERIC NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS token_hour_data_current ON "token_hour_data" (id) WHERE upper_inf(block_range);

CREATE INDEX IF NOT EXISTS token_hour_data_history ON "token_hour_data" USING gist(block_range);

CREATE INDEX IF NOT EXISTS token_hour_data_token ON "token_hour_data" ("token");

CREATE OR REPLACE VIEW pool_state AS
SELECT 'Factory'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'poolCount', "pool_count"::TEXT, 'txCount', "tx_count"::TEXT, 'totalVolumeUSD', "total_volume_usd"::TEXT, 'totalVolumeETH', "total_volume_eth"::TEXT, 'totalFeesUSD', "total_fees_usd"::TEXT, 'totalFeesETH', "total_fees_eth"::TEXT, 'untrackedVolumeUSD', "untracked_volume_usd"::TEXT, 'totalValueLockedUSD', "total_value_locked_usd"::TEXT, 'totalValueLockedETH', "total_value_locked_eth"::TEXT, 'totalValueLockedUSDUntracked', "total_value_locked_usd_untracked"::TEXT, 'totalValueLockedETHUntracked', "total_value_locked_eth_untracked"::TEXT, 'owner', "owner") AS data, lower(block_range) AS block_number FROM "factory" WHERE upper_inf(block_range)
UNION ALL
SELECT 'Bundle'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'ethPriceUSD', "eth_price_usd"::TEXT) AS data, lower(block_range) AS block_number FROM "bundle" WHERE upper_inf(block_range)
UNION ALL
SELECT 'Token'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'symbol', "symbol", 'name', "name", 'decimals', "decimals"::TEXT, 'totalSupply', "total_supply"::TEXT, 'volume', "volume"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'untrackedVolumeUSD', "untracked_volume_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'txCount', "tx_count"::TEXT, 'poolCount', "pool_count"::TEXT, 'totalValueLocked', "total_value_locked"::TEXT, 'totalValueLockedUSD', "total_value_locked_usd"::TEXT, 'totalValueLockedUSDUntracked', "total_value_locked_usd_untracked"::TEXT, 'derivedETH', "derived_eth"::TEXT, 'whitelistPools', "whitelist_pools") AS data, lower(block_range) AS block_number FROM "token" WHERE upper_inf(block_range)
UNION ALL
SELECT 'Pool'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'createdAtTimestamp', "created_at_timestamp"::TEXT, 'createdAtBlockNumber', "created_at_block_number"::TEXT, 'token0', "token0", 'token1', "token1", 'feeTier', "fee_tier"::TEXT, 'liquidity', "liquidity"::TEXT, 'sqrtPrice', "sqrt_price"::TEXT, 'feeGrowthGlobal0X128', "fee_growth_global0_x128"::TEXT, 'feeGrowthGlobal1X128', "fee_growth_global1_x128"::TEXT, 'token0Price', "token0_price"::TEXT, 'token1Price', "token1_price"::TEXT, 'tick', "tick"::TEXT, 'observationIndex', "observation_index"::TEXT, 'volumeToken0', "volume_token0"::TEXT, 'volumeToken1', "volume_token1"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'untrackedVolumeUSD', "untracked_volume_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'txCount', "tx_count"::TEXT, 'collectedFeesToken0', "collected_fees_token0"::TEXT, 'collectedFeesToken1', "collected_fees_token1"::TEXT, 'collectedFeesUSD', "collected_fees_usd"::TEXT, 'totalValueLockedToken0', "total_value_locked_token0"::TEXT, 'totalValueLockedToken1', "total_value_locked_token1"::TEXT, 'totalValueLockedETH', "total_value_locked_eth"::TEXT, 'totalValueLockedUSD', "total_value_locked_usd"::TEXT, 'totalValueLockedUSDUntracked', "total_value_locked_usd_untracked"::TEXT, 'liquidityProviderCount', "liquidity_provider_count"::TEXT) AS data, lower(block_range) AS block_number FROM "pool" WHERE upper_inf(block_range)
UNION ALL
SELECT 'Tick'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'poolAddress', "pool_address", 'tickIdx', "tick_idx"::TEXT, 'pool', "pool", 'liquidityGross', "liquidity_gross"::TEXT, 'liquidityNet', "liquidity_net"::TEXT, 'price0', "price0"::TEXT, 'price1', "price1"::TEXT, 'volumeToken0', "volume_token0"::TEXT, 'volumeToken1', "volume_token1"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'untrackedVolumeUSD', "untracked_volume_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'collectedFeesToken0', "collected_fees_token0"::TEXT, 'collectedFeesToken1', "collected_fees_token1"::TEXT, 'collectedFeesUSD', "collected_fees_usd"::TEXT, 'createdAtTimestamp', "created_at_timestamp"::TEXT, 'createdAtBlockNumber', "created_at_block_number"::TEXT, 'liquidityProviderCount', "liquidity_provider_count"::TEXT, 'feeGrowthOutside0X128', "fee_growth_outside0_x128"::TEXT, 'feeGrowthOutside1X128', "fee_growth_outside1_x128"::TEXT) AS data, lower(block_range) AS block_number FROM "tick" WHERE upper_inf(block_range)
UNION ALL
SELECT 'Position'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'owner', "owner", 'pool', "pool", 'token0', "token0", 'token1', "token1", 'tickLower', "tick_lower", 'tickUpper', "tick_upper", 'liquidity', "liquidity"::TEXT, 'depositedToken0', "deposited_token0"::TEXT, 'depositedToken1', "deposited_token1"::TEXT, 'withdrawnToken0', "withdrawn_token0"::TEXT, 'withdrawnToken1', "withdrawn_token1"::TEXT, 'collectedFeesToken0', "collected_fees_token0"::TEXT, 'collectedFeesToken1', "collected_fees_token1"::TEXT, 'transaction', "transaction", 'feeGrowthInside0LastX128', "fee_growth_inside0_last_x128"::TEXT, 'feeGrowthInside1LastX128', "fee_growth_inside1_last_x128"::TEXT) AS data, lower(block_range) AS block_number FROM "position" WHERE upper_inf(block_range)
UNION ALL
SELECT 'PositionSnapshot'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'owner', "owner", 'pool', "pool", 'position', "position", 'blockNumber', "block_number"::TEXT, 'timestamp', "timestamp"::TEXT, 'liquidity', "liquidity"::TEXT, 'depositedToken0', "deposited_token0"::TEXT, 'depositedToken1', "deposited_token1"::TEXT, 'withdrawnToken0', "withdrawn_token0"::TEXT, 'withdrawnToken1', "withdrawn_token1"::TEXT, 'collectedFeesToken0', "collected_fees_token0"::TEXT, 'collectedFeesToken1', "collected_fees_token1"::TEXT, 'transaction', "transaction", 'feeGrowthInside0LastX128', "fee_growth_inside0_last_x128"::TEXT, 'feeGrowthInside1LastX128', "fee_growth_inside1_last_x128"::TEXT) AS data, lower(block_range) AS block_number FROM "position_snapshot" WHERE upper_inf(block_range)
UNION ALL
SELECT 'Transaction'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'blockNumber', "block_number"::TEXT, 'timestamp', "timestamp"::TEXT, 'gasUsed', "gas_used"::TEXT, 'gasPrice', "gas_price"::TEXT) AS data, block_number AS block_number FROM "transaction"
UNION ALL
SELECT 'Mint'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'transaction', "transaction", 'timestamp', "timestamp"::TEXT, 'pool', "pool", 'token0', "token0", 'token1', "token1", 'owner', "owner", 'sender', "sender", 'origin', "origin", 'amount', "amount"::TEXT, 'amount0', "amount0"::TEXT, 'amount1', "amount1"::TEXT, 'amountUSD', "amount_usd"::TEXT, 'tickLower', "tick_lower"::TEXT, 'tickUpper', "tick_upper"::TEXT, 'logIndex', "log_index"::TEXT) AS data, block_number AS block_number FROM "mint"
UNION ALL
SELECT 'Burn'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'transaction', "transaction", 'pool', "pool", 'token0', "token0", 'token1', "token1", 'timestamp', "timestamp"::TEXT, 'owner', "owner", 'origin', "origin", 'amount', "amount"::TEXT, 'amount0', "amount0"::TEXT, 'amount1', "amount1"::TEXT, 'amountUSD', "amount_usd"::TEXT, 'tickLower', "tick_lower"::TEXT, 'tickUpper', "tick_upper"::TEXT, 'logIndex', "log_index"::TEXT) AS data, block_number AS block_number FROM "burn"
UNION ALL
SELECT 'Swap'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'transaction', "transaction", 'timestamp', "timestamp"::TEXT, 'pool', "pool", 'token0', "token0", 'token1', "token1", 'sender', "sender", 'recipient', "recipient", 'origin', "origin", 'amount0', "amount0"::TEXT, 'amount1', "amount1"::TEXT, 'amountUSD', "amount_usd"::TEXT, 'sqrtPriceX96', "sqrt_price_x96"::TEXT, 'tick', "tick"::TEXT, 'logIndex', "log_index"::TEXT) AS data, block_number AS block_number FROM "swap"
UNION ALL
SELECT 'Collect'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'transaction', "transaction", 'timestamp', "timestamp"::TEXT, 'pool', "pool", 'owner', "owner", 'amount0', "amount0"::TEXT, 'amount1', "amount1"::TEXT, 'amountUSD', "amount_usd"::TEXT, 'tickLower', "tick_lower"::TEXT, 'tickUpper', "tick_upper"::TEXT, 'logIndex', "log_index"::TEXT) AS data, block_number AS block_number FROM "collect"
UNION ALL
SELECT 'Flash'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'transaction', "transaction", 'timestamp', "timestamp"::TEXT, 'pool', "pool", 'sender', "sender", 'recipient', "recipient", 'amount0', "amount0"::TEXT, 'amount1', "amount1"::TEXT, 'amountUSD', "amount_usd"::TEXT, 'amount0Paid', "amount0_paid"::TEXT, 'amount1Paid', "amount1_paid"::TEXT, 'logIndex', "log_index"::TEXT) AS data, block_number AS block_number FROM "flash"
UNION ALL
SELECT 'UniswapDayData'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'date', "date", 'volumeETH', "volume_eth"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'volumeUSDUntracked', "volume_usd_untracked"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'txCount', "tx_count"::TEXT, 'tvlUSD', "tvl_usd"::TEXT) AS data, lower(block_range) AS block_number FROM "uniswap_day_data" WHERE upper_inf(block_range)
UNION ALL
SELECT 'PoolDayData'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'date', "date", 'pool', "pool", 'liquidity', "liquidity"::TEXT, 'sqrtPrice', "sqrt_price"::TEXT, 'token0Price', "token0_price"::TEXT, 'token1Price', "token1_price"::TEXT, 'tick', "tick"::TEXT, 'feeGrowthGlobal0X128', "fee_growth_global0_x128"::TEXT, 'feeGrowthGlobal1X128', "fee_growth_global1_x128"::TEXT, 'tvlUSD', "tvl_usd"::TEXT, 'volumeToken0', "volume_token0"::TEXT, 'volumeToken1', "volume_token1"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'txCount', "tx_count"::TEXT, 'open', "open"::TEXT, 'high', "high"::TEXT, 'low', "low"::TEXT, 'close', "close"::TEXT) AS data, lower(block_range) AS block_number FROM "pool_day_data" WHERE upper_inf(block_range)
UNION ALL
SELECT 'PoolHourData'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'periodStartUnix', "period_start_unix", 'pool', "pool", 'liquidity', "liquidity"::TEXT, 'sqrtPrice', "sqrt_price"::TEXT, 'token0Price', "token0_price"::TEXT, 'token1Price', "token1_price"::TEXT, 'tick', "tick"::TEXT, 'feeGrowthGlobal0X128', "fee_growth_global0_x128"::TEXT, 'feeGrowthGlobal1X128', "fee_growth_global1_x128"::TEXT, 'tvlUSD', "tvl_usd"::TEXT, 'volumeToken0', "volume_token0"::TEXT, 'volumeToken1', "volume_token1"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'txCount', "tx_count"::TEXT, 'open', "open"::TEXT, 'high', "high"::TEXT, 'low', "low"::TEXT, 'close', "close"::TEXT) AS data, lower(block_range) AS block_number FROM "pool_hour_data" WHERE upper_inf(block_range)
UNION ALL
SELECT 'TickHourData'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'periodStartUnix', "period_start_unix", 'pool', "pool", 'tick', "tick", 'liquidityGross', "liquidity_gross"::TEXT, 'liquidityNet', "liquidity_net"::TEXT, 'volumeToken0', "volume_token0"::TEXT, 'volumeToken1', "volume_token1"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT) AS data, lower(block_range) AS block_number FROM "tick_hour_data" WHERE upper_inf(block_range)
UNION ALL
SELECT 'TickDayData'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'date', "date", 'pool', "pool", 'tick', "tick", 'liquidityGross', "liquidity_gross"::TEXT, 'liquidityNet', "liquidity_net"::TEXT, 'volumeToken0', "volume_token0"::TEXT, 'volumeToken1', "volume_token1"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'feeGrowthOutside0X128', "fee_growth_outside0_x128"::TEXT, 'feeGrowthOutside1X128', "fee_growth_outside1_x128"::TEXT) AS data, lower(block_range) AS block_number FROM "tick_day_data" WHERE upper_inf(block_range)
UNION ALL
SELECT 'TokenDayData'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'date', "date", 'token', "token", 'volume', "volume"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'untrackedVolumeUSD', "untracked_volume_usd"::TEXT, 'totalValueLocked', "total_value_locked"::TEXT, 'totalValueLockedUSD', "total_value_locked_usd"::TEXT, 'priceUSD', "price_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'open', "open"::TEXT, 'high', "high"::TEXT, 'low', "low"::TEXT, 'close', "close"::TEXT) AS data, lower(block_range) AS block_number FROM "token_day_data" WHERE upper_inf(block_range)
UNION ALL
SELECT 'TokenHourData'::TEXT AS entity_type, id AS entity_id, jsonb_build_object('id', "id", 'periodStartUnix', "period_start_unix", 'token', "token", 'volume', "volume"::TEXT, 'volumeUSD', "volume_usd"::TEXT, 'untrackedVolumeUSD', "untracked_volume_usd"::TEXT, 'totalValueLocked', "total_value_locked"::TEXT, 'totalValueLockedUSD', "total_value_locked_usd"::TEXT, 'priceUSD', "price_usd"::TEXT, 'feesUSD', "fees_usd"::TEXT, 'open', "open"::TEXT, 'high', "high"::TEXT, 'low', "low"::TEXT, 'close', "close"::TEXT) AS data, lower(block_range) AS block_number FROM "token_hour_data" WHERE upper_inf(block_range);

-- Applies final Uniswap entity changes at height within the caller's transaction.
-- Mutable rows retain block ranges; immutable entities cannot change or be deleted.
CREATE OR REPLACE FUNCTION pool_apply(height BIGINT, changes JSONB) RETURNS VOID
LANGUAGE plpgsql SET search_path FROM CURRENT AS $apply$
DECLARE change JSONB; data JSONB; previous JSONB;
BEGIN
    FOR change IN SELECT * FROM jsonb_array_elements(changes) LOOP
        data := change->'data';
        IF data = 'null'::JSONB THEN data := NULL; END IF;
        CASE change->>'kind'
    WHEN 'Factory' THEN
    DELETE FROM "factory" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "factory" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "factory" (block_range, "id", "pool_count", "tx_count", "total_volume_usd", "total_volume_eth", "total_fees_usd", "total_fees_eth", "untracked_volume_usd", "total_value_locked_usd", "total_value_locked_eth", "total_value_locked_usd_untracked", "total_value_locked_eth_untracked", "owner") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'poolCount')::NUMERIC, (data->>'txCount')::NUMERIC, (data->>'totalVolumeUSD')::NUMERIC, (data->>'totalVolumeETH')::NUMERIC, (data->>'totalFeesUSD')::NUMERIC, (data->>'totalFeesETH')::NUMERIC, (data->>'untrackedVolumeUSD')::NUMERIC, (data->>'totalValueLockedUSD')::NUMERIC, (data->>'totalValueLockedETH')::NUMERIC, (data->>'totalValueLockedUSDUntracked')::NUMERIC, (data->>'totalValueLockedETHUntracked')::NUMERIC, (data->>'owner')::TEXT);
    END IF;
    WHEN 'Bundle' THEN
    DELETE FROM "bundle" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "bundle" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "bundle" (block_range, "id", "eth_price_usd") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'ethPriceUSD')::NUMERIC);
    END IF;
    WHEN 'Token' THEN
    DELETE FROM "token" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "token" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "token" (block_range, "id", "symbol", "name", "decimals", "total_supply", "volume", "volume_usd", "untracked_volume_usd", "fees_usd", "tx_count", "pool_count", "total_value_locked", "total_value_locked_usd", "total_value_locked_usd_untracked", "derived_eth", "whitelist_pools") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'symbol')::TEXT, (data->>'name')::TEXT, (data->>'decimals')::NUMERIC, (data->>'totalSupply')::NUMERIC, (data->>'volume')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'untrackedVolumeUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'txCount')::NUMERIC, (data->>'poolCount')::NUMERIC, (data->>'totalValueLocked')::NUMERIC, (data->>'totalValueLockedUSD')::NUMERIC, (data->>'totalValueLockedUSDUntracked')::NUMERIC, (data->>'derivedETH')::NUMERIC, ARRAY(SELECT jsonb_array_elements_text(data->'whitelistPools')));
    END IF;
    WHEN 'Pool' THEN
    DELETE FROM "pool" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "pool" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "pool" (block_range, "id", "created_at_timestamp", "created_at_block_number", "token0", "token1", "fee_tier", "liquidity", "sqrt_price", "fee_growth_global0_x128", "fee_growth_global1_x128", "token0_price", "token1_price", "tick", "observation_index", "volume_token0", "volume_token1", "volume_usd", "untracked_volume_usd", "fees_usd", "tx_count", "collected_fees_token0", "collected_fees_token1", "collected_fees_usd", "total_value_locked_token0", "total_value_locked_token1", "total_value_locked_eth", "total_value_locked_usd", "total_value_locked_usd_untracked", "liquidity_provider_count") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'createdAtTimestamp')::NUMERIC, (data->>'createdAtBlockNumber')::BIGINT, (data->>'token0')::TEXT, (data->>'token1')::TEXT, (data->>'feeTier')::NUMERIC, (data->>'liquidity')::NUMERIC, (data->>'sqrtPrice')::NUMERIC, (data->>'feeGrowthGlobal0X128')::NUMERIC, (data->>'feeGrowthGlobal1X128')::NUMERIC, (data->>'token0Price')::NUMERIC, (data->>'token1Price')::NUMERIC, (data->>'tick')::NUMERIC, (data->>'observationIndex')::NUMERIC, (data->>'volumeToken0')::NUMERIC, (data->>'volumeToken1')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'untrackedVolumeUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'txCount')::NUMERIC, (data->>'collectedFeesToken0')::NUMERIC, (data->>'collectedFeesToken1')::NUMERIC, (data->>'collectedFeesUSD')::NUMERIC, (data->>'totalValueLockedToken0')::NUMERIC, (data->>'totalValueLockedToken1')::NUMERIC, (data->>'totalValueLockedETH')::NUMERIC, (data->>'totalValueLockedUSD')::NUMERIC, (data->>'totalValueLockedUSDUntracked')::NUMERIC, (data->>'liquidityProviderCount')::NUMERIC);
    END IF;
    WHEN 'Tick' THEN
    DELETE FROM "tick" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "tick" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "tick" (block_range, "id", "pool_address", "tick_idx", "pool", "liquidity_gross", "liquidity_net", "price0", "price1", "volume_token0", "volume_token1", "volume_usd", "untracked_volume_usd", "fees_usd", "collected_fees_token0", "collected_fees_token1", "collected_fees_usd", "created_at_timestamp", "created_at_block_number", "liquidity_provider_count", "fee_growth_outside0_x128", "fee_growth_outside1_x128") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'poolAddress')::TEXT, (data->>'tickIdx')::NUMERIC, (data->>'pool')::TEXT, (data->>'liquidityGross')::NUMERIC, (data->>'liquidityNet')::NUMERIC, (data->>'price0')::NUMERIC, (data->>'price1')::NUMERIC, (data->>'volumeToken0')::NUMERIC, (data->>'volumeToken1')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'untrackedVolumeUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'collectedFeesToken0')::NUMERIC, (data->>'collectedFeesToken1')::NUMERIC, (data->>'collectedFeesUSD')::NUMERIC, (data->>'createdAtTimestamp')::NUMERIC, (data->>'createdAtBlockNumber')::BIGINT, (data->>'liquidityProviderCount')::NUMERIC, (data->>'feeGrowthOutside0X128')::NUMERIC, (data->>'feeGrowthOutside1X128')::NUMERIC);
    END IF;
    WHEN 'Position' THEN
    DELETE FROM "position" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "position" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "position" (block_range, "id", "owner", "pool", "token0", "token1", "tick_lower", "tick_upper", "liquidity", "deposited_token0", "deposited_token1", "withdrawn_token0", "withdrawn_token1", "collected_fees_token0", "collected_fees_token1", "transaction", "fee_growth_inside0_last_x128", "fee_growth_inside1_last_x128") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'owner')::TEXT, (data->>'pool')::TEXT, (data->>'token0')::TEXT, (data->>'token1')::TEXT, (data->>'tickLower')::TEXT, (data->>'tickUpper')::TEXT, (data->>'liquidity')::NUMERIC, (data->>'depositedToken0')::NUMERIC, (data->>'depositedToken1')::NUMERIC, (data->>'withdrawnToken0')::NUMERIC, (data->>'withdrawnToken1')::NUMERIC, (data->>'collectedFeesToken0')::NUMERIC, (data->>'collectedFeesToken1')::NUMERIC, (data->>'transaction')::TEXT, (data->>'feeGrowthInside0LastX128')::NUMERIC, (data->>'feeGrowthInside1LastX128')::NUMERIC);
    END IF;
    WHEN 'PositionSnapshot' THEN
    DELETE FROM "position_snapshot" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "position_snapshot" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "position_snapshot" (block_range, "id", "owner", "pool", "position", "block_number", "timestamp", "liquidity", "deposited_token0", "deposited_token1", "withdrawn_token0", "withdrawn_token1", "collected_fees_token0", "collected_fees_token1", "transaction", "fee_growth_inside0_last_x128", "fee_growth_inside1_last_x128") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'owner')::TEXT, (data->>'pool')::TEXT, (data->>'position')::TEXT, (data->>'blockNumber')::BIGINT, (data->>'timestamp')::NUMERIC, (data->>'liquidity')::NUMERIC, (data->>'depositedToken0')::NUMERIC, (data->>'depositedToken1')::NUMERIC, (data->>'withdrawnToken0')::NUMERIC, (data->>'withdrawnToken1')::NUMERIC, (data->>'collectedFeesToken0')::NUMERIC, (data->>'collectedFeesToken1')::NUMERIC, (data->>'transaction')::TEXT, (data->>'feeGrowthInside0LastX128')::NUMERIC, (data->>'feeGrowthInside1LastX128')::NUMERIC);
    END IF;
    WHEN 'Transaction' THEN
    SELECT state.data INTO previous FROM pool_state state WHERE entity_type = 'Transaction' AND entity_id = (change->>'id');
    IF data IS NULL OR (previous IS NOT NULL AND previous <> data) THEN RAISE EXCEPTION 'immutable Uniswap entity cannot change'; END IF;
    IF previous IS NOT NULL THEN CONTINUE; END IF;
    INSERT INTO "transaction" ("id", "block_number", "timestamp", "gas_used", "gas_price") VALUES ((data->>'id')::TEXT, (data->>'blockNumber')::BIGINT, (data->>'timestamp')::NUMERIC, (data->>'gasUsed')::NUMERIC, (data->>'gasPrice')::NUMERIC);
    WHEN 'Mint' THEN
    SELECT state.data INTO previous FROM pool_state state WHERE entity_type = 'Mint' AND entity_id = (change->>'id');
    IF data IS NULL OR (previous IS NOT NULL AND previous <> data) THEN RAISE EXCEPTION 'immutable Uniswap entity cannot change'; END IF;
    IF previous IS NOT NULL THEN CONTINUE; END IF;
    INSERT INTO "mint" (block_number, "id", "transaction", "timestamp", "pool", "token0", "token1", "owner", "sender", "origin", "amount", "amount0", "amount1", "amount_usd", "tick_lower", "tick_upper", "log_index") VALUES (height, (data->>'id')::TEXT, (data->>'transaction')::TEXT, (data->>'timestamp')::NUMERIC, (data->>'pool')::TEXT, (data->>'token0')::TEXT, (data->>'token1')::TEXT, (data->>'owner')::TEXT, (data->>'sender')::TEXT, (data->>'origin')::TEXT, (data->>'amount')::NUMERIC, (data->>'amount0')::NUMERIC, (data->>'amount1')::NUMERIC, (data->>'amountUSD')::NUMERIC, (data->>'tickLower')::NUMERIC, (data->>'tickUpper')::NUMERIC, (data->>'logIndex')::NUMERIC);
    WHEN 'Burn' THEN
    SELECT state.data INTO previous FROM pool_state state WHERE entity_type = 'Burn' AND entity_id = (change->>'id');
    IF data IS NULL OR (previous IS NOT NULL AND previous <> data) THEN RAISE EXCEPTION 'immutable Uniswap entity cannot change'; END IF;
    IF previous IS NOT NULL THEN CONTINUE; END IF;
    INSERT INTO "burn" (block_number, "id", "transaction", "pool", "token0", "token1", "timestamp", "owner", "origin", "amount", "amount0", "amount1", "amount_usd", "tick_lower", "tick_upper", "log_index") VALUES (height, (data->>'id')::TEXT, (data->>'transaction')::TEXT, (data->>'pool')::TEXT, (data->>'token0')::TEXT, (data->>'token1')::TEXT, (data->>'timestamp')::NUMERIC, (data->>'owner')::TEXT, (data->>'origin')::TEXT, (data->>'amount')::NUMERIC, (data->>'amount0')::NUMERIC, (data->>'amount1')::NUMERIC, (data->>'amountUSD')::NUMERIC, (data->>'tickLower')::NUMERIC, (data->>'tickUpper')::NUMERIC, (data->>'logIndex')::NUMERIC);
    WHEN 'Swap' THEN
    SELECT state.data INTO previous FROM pool_state state WHERE entity_type = 'Swap' AND entity_id = (change->>'id');
    IF data IS NULL OR (previous IS NOT NULL AND previous <> data) THEN RAISE EXCEPTION 'immutable Uniswap entity cannot change'; END IF;
    IF previous IS NOT NULL THEN CONTINUE; END IF;
    INSERT INTO "swap" (block_number, "id", "transaction", "timestamp", "pool", "token0", "token1", "sender", "recipient", "origin", "amount0", "amount1", "amount_usd", "sqrt_price_x96", "tick", "log_index") VALUES (height, (data->>'id')::TEXT, (data->>'transaction')::TEXT, (data->>'timestamp')::NUMERIC, (data->>'pool')::TEXT, (data->>'token0')::TEXT, (data->>'token1')::TEXT, (data->>'sender')::TEXT, (data->>'recipient')::TEXT, (data->>'origin')::TEXT, (data->>'amount0')::NUMERIC, (data->>'amount1')::NUMERIC, (data->>'amountUSD')::NUMERIC, (data->>'sqrtPriceX96')::NUMERIC, (data->>'tick')::NUMERIC, (data->>'logIndex')::NUMERIC);
    WHEN 'Collect' THEN
    SELECT state.data INTO previous FROM pool_state state WHERE entity_type = 'Collect' AND entity_id = (change->>'id');
    IF data IS NULL OR (previous IS NOT NULL AND previous <> data) THEN RAISE EXCEPTION 'immutable Uniswap entity cannot change'; END IF;
    IF previous IS NOT NULL THEN CONTINUE; END IF;
    INSERT INTO "collect" (block_number, "id", "transaction", "timestamp", "pool", "owner", "amount0", "amount1", "amount_usd", "tick_lower", "tick_upper", "log_index") VALUES (height, (data->>'id')::TEXT, (data->>'transaction')::TEXT, (data->>'timestamp')::NUMERIC, (data->>'pool')::TEXT, (data->>'owner')::TEXT, (data->>'amount0')::NUMERIC, (data->>'amount1')::NUMERIC, (data->>'amountUSD')::NUMERIC, (data->>'tickLower')::NUMERIC, (data->>'tickUpper')::NUMERIC, (data->>'logIndex')::NUMERIC);
    WHEN 'Flash' THEN
    SELECT state.data INTO previous FROM pool_state state WHERE entity_type = 'Flash' AND entity_id = (change->>'id');
    IF data IS NULL OR (previous IS NOT NULL AND previous <> data) THEN RAISE EXCEPTION 'immutable Uniswap entity cannot change'; END IF;
    IF previous IS NOT NULL THEN CONTINUE; END IF;
    INSERT INTO "flash" (block_number, "id", "transaction", "timestamp", "pool", "sender", "recipient", "amount0", "amount1", "amount_usd", "amount0_paid", "amount1_paid", "log_index") VALUES (height, (data->>'id')::TEXT, (data->>'transaction')::TEXT, (data->>'timestamp')::NUMERIC, (data->>'pool')::TEXT, (data->>'sender')::TEXT, (data->>'recipient')::TEXT, (data->>'amount0')::NUMERIC, (data->>'amount1')::NUMERIC, (data->>'amountUSD')::NUMERIC, (data->>'amount0Paid')::NUMERIC, (data->>'amount1Paid')::NUMERIC, (data->>'logIndex')::NUMERIC);
    WHEN 'UniswapDayData' THEN
    DELETE FROM "uniswap_day_data" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "uniswap_day_data" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "uniswap_day_data" (block_range, "id", "date", "volume_eth", "volume_usd", "volume_usd_untracked", "fees_usd", "tx_count", "tvl_usd") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'date')::INTEGER, (data->>'volumeETH')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'volumeUSDUntracked')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'txCount')::NUMERIC, (data->>'tvlUSD')::NUMERIC);
    END IF;
    WHEN 'PoolDayData' THEN
    DELETE FROM "pool_day_data" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "pool_day_data" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "pool_day_data" (block_range, "id", "date", "pool", "liquidity", "sqrt_price", "token0_price", "token1_price", "tick", "fee_growth_global0_x128", "fee_growth_global1_x128", "tvl_usd", "volume_token0", "volume_token1", "volume_usd", "fees_usd", "tx_count", "open", "high", "low", "close") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'date')::INTEGER, (data->>'pool')::TEXT, (data->>'liquidity')::NUMERIC, (data->>'sqrtPrice')::NUMERIC, (data->>'token0Price')::NUMERIC, (data->>'token1Price')::NUMERIC, (data->>'tick')::NUMERIC, (data->>'feeGrowthGlobal0X128')::NUMERIC, (data->>'feeGrowthGlobal1X128')::NUMERIC, (data->>'tvlUSD')::NUMERIC, (data->>'volumeToken0')::NUMERIC, (data->>'volumeToken1')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'txCount')::NUMERIC, (data->>'open')::NUMERIC, (data->>'high')::NUMERIC, (data->>'low')::NUMERIC, (data->>'close')::NUMERIC);
    END IF;
    WHEN 'PoolHourData' THEN
    DELETE FROM "pool_hour_data" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "pool_hour_data" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "pool_hour_data" (block_range, "id", "period_start_unix", "pool", "liquidity", "sqrt_price", "token0_price", "token1_price", "tick", "fee_growth_global0_x128", "fee_growth_global1_x128", "tvl_usd", "volume_token0", "volume_token1", "volume_usd", "fees_usd", "tx_count", "open", "high", "low", "close") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'periodStartUnix')::INTEGER, (data->>'pool')::TEXT, (data->>'liquidity')::NUMERIC, (data->>'sqrtPrice')::NUMERIC, (data->>'token0Price')::NUMERIC, (data->>'token1Price')::NUMERIC, (data->>'tick')::NUMERIC, (data->>'feeGrowthGlobal0X128')::NUMERIC, (data->>'feeGrowthGlobal1X128')::NUMERIC, (data->>'tvlUSD')::NUMERIC, (data->>'volumeToken0')::NUMERIC, (data->>'volumeToken1')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'txCount')::NUMERIC, (data->>'open')::NUMERIC, (data->>'high')::NUMERIC, (data->>'low')::NUMERIC, (data->>'close')::NUMERIC);
    END IF;
    WHEN 'TickHourData' THEN
    DELETE FROM "tick_hour_data" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "tick_hour_data" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "tick_hour_data" (block_range, "id", "period_start_unix", "pool", "tick", "liquidity_gross", "liquidity_net", "volume_token0", "volume_token1", "volume_usd", "fees_usd") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'periodStartUnix')::INTEGER, (data->>'pool')::TEXT, (data->>'tick')::TEXT, (data->>'liquidityGross')::NUMERIC, (data->>'liquidityNet')::NUMERIC, (data->>'volumeToken0')::NUMERIC, (data->>'volumeToken1')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC);
    END IF;
    WHEN 'TickDayData' THEN
    DELETE FROM "tick_day_data" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "tick_day_data" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "tick_day_data" (block_range, "id", "date", "pool", "tick", "liquidity_gross", "liquidity_net", "volume_token0", "volume_token1", "volume_usd", "fees_usd", "fee_growth_outside0_x128", "fee_growth_outside1_x128") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'date')::INTEGER, (data->>'pool')::TEXT, (data->>'tick')::TEXT, (data->>'liquidityGross')::NUMERIC, (data->>'liquidityNet')::NUMERIC, (data->>'volumeToken0')::NUMERIC, (data->>'volumeToken1')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'feeGrowthOutside0X128')::NUMERIC, (data->>'feeGrowthOutside1X128')::NUMERIC);
    END IF;
    WHEN 'TokenDayData' THEN
    DELETE FROM "token_day_data" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "token_day_data" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "token_day_data" (block_range, "id", "date", "token", "volume", "volume_usd", "untracked_volume_usd", "total_value_locked", "total_value_locked_usd", "price_usd", "fees_usd", "open", "high", "low", "close") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'date')::INTEGER, (data->>'token')::TEXT, (data->>'volume')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'untrackedVolumeUSD')::NUMERIC, (data->>'totalValueLocked')::NUMERIC, (data->>'totalValueLockedUSD')::NUMERIC, (data->>'priceUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'open')::NUMERIC, (data->>'high')::NUMERIC, (data->>'low')::NUMERIC, (data->>'close')::NUMERIC);
    END IF;
    WHEN 'TokenHourData' THEN
    DELETE FROM "token_hour_data" WHERE id = (change->>'id') AND lower(block_range) = height;
    UPDATE "token_hour_data" SET block_range = int8range(lower(block_range), height, '[)') WHERE id = (change->>'id') AND upper_inf(block_range);
    IF data IS NOT NULL THEN
    INSERT INTO "token_hour_data" (block_range, "id", "period_start_unix", "token", "volume", "volume_usd", "untracked_volume_usd", "total_value_locked", "total_value_locked_usd", "price_usd", "fees_usd", "open", "high", "low", "close") VALUES (int8range(height, NULL, '[)'), (data->>'id')::TEXT, (data->>'periodStartUnix')::INTEGER, (data->>'token')::TEXT, (data->>'volume')::NUMERIC, (data->>'volumeUSD')::NUMERIC, (data->>'untrackedVolumeUSD')::NUMERIC, (data->>'totalValueLocked')::NUMERIC, (data->>'totalValueLockedUSD')::NUMERIC, (data->>'priceUSD')::NUMERIC, (data->>'feesUSD')::NUMERIC, (data->>'open')::NUMERIC, (data->>'high')::NUMERIC, (data->>'low')::NUMERIC, (data->>'close')::NUMERIC);
    END IF;
        ELSE RAISE EXCEPTION 'unknown Uniswap entity';
        END CASE;
    END LOOP;
END; $apply$;

-- Removes versions and immutable rows from height onward, then reopens prior ranges.
CREATE OR REPLACE FUNCTION pool_revert(height BIGINT) RETURNS VOID
LANGUAGE plpgsql SET search_path FROM CURRENT AS $revert$
BEGIN
DELETE FROM "factory" WHERE lower(block_range) >= height;
UPDATE "factory" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "bundle" WHERE lower(block_range) >= height;
UPDATE "bundle" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "token" WHERE lower(block_range) >= height;
UPDATE "token" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "pool" WHERE lower(block_range) >= height;
UPDATE "pool" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "tick" WHERE lower(block_range) >= height;
UPDATE "tick" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "position" WHERE lower(block_range) >= height;
UPDATE "position" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "position_snapshot" WHERE lower(block_range) >= height;
UPDATE "position_snapshot" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "transaction" WHERE block_number >= height;
DELETE FROM "mint" WHERE block_number >= height;
DELETE FROM "burn" WHERE block_number >= height;
DELETE FROM "swap" WHERE block_number >= height;
DELETE FROM "collect" WHERE block_number >= height;
DELETE FROM "flash" WHERE block_number >= height;
DELETE FROM "uniswap_day_data" WHERE lower(block_range) >= height;
UPDATE "uniswap_day_data" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "pool_day_data" WHERE lower(block_range) >= height;
UPDATE "pool_day_data" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "pool_hour_data" WHERE lower(block_range) >= height;
UPDATE "pool_hour_data" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "tick_hour_data" WHERE lower(block_range) >= height;
UPDATE "tick_hour_data" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "tick_day_data" WHERE lower(block_range) >= height;
UPDATE "tick_day_data" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "token_day_data" WHERE lower(block_range) >= height;
UPDATE "token_day_data" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
DELETE FROM "token_hour_data" WHERE lower(block_range) >= height;
UPDATE "token_hour_data" SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
END; $revert$;
