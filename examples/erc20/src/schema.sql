-- Application-owned native entity tables and historical versions.
CREATE TABLE IF NOT EXISTS account (
    vid BIGSERIAL PRIMARY KEY, block_range INT8RANGE NOT NULL,
    id TEXT NOT NULL, as_erc20 TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS account_current ON account (id) WHERE upper_inf(block_range);
CREATE INDEX IF NOT EXISTS account_history ON account USING gist (block_range);

CREATE TABLE IF NOT EXISTS erc20_contract (
    vid BIGSERIAL PRIMARY KEY, block_number BIGINT NOT NULL,
    id TEXT NOT NULL UNIQUE, as_account TEXT NOT NULL, name TEXT, symbol TEXT,
    decimals INTEGER NOT NULL, total_supply TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS erc20_contract_block ON erc20_contract (block_number);

CREATE TABLE IF NOT EXISTS erc20_balance (
    vid BIGSERIAL PRIMARY KEY, block_range INT8RANGE NOT NULL,
    id TEXT NOT NULL, contract TEXT NOT NULL, account TEXT,
    value NUMERIC NOT NULL, value_exact NUMERIC NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS erc20_balance_current ON erc20_balance (id) WHERE upper_inf(block_range);
CREATE INDEX IF NOT EXISTS erc20_balance_history ON erc20_balance USING gist (block_range);
CREATE INDEX IF NOT EXISTS erc20_balance_account ON erc20_balance (account);
CREATE INDEX IF NOT EXISTS erc20_balance_contract ON erc20_balance (contract);

CREATE TABLE IF NOT EXISTS erc20_transfer (
    vid BIGSERIAL PRIMARY KEY, block_number BIGINT NOT NULL,
    id TEXT NOT NULL UNIQUE, emitter TEXT NOT NULL, transaction TEXT NOT NULL,
    timestamp NUMERIC NOT NULL, contract TEXT NOT NULL,
    "from" TEXT, from_balance TEXT, "to" TEXT, to_balance TEXT,
    value NUMERIC NOT NULL, value_exact NUMERIC NOT NULL
);
CREATE INDEX IF NOT EXISTS erc20_transfer_block ON erc20_transfer (block_number);
CREATE INDEX IF NOT EXISTS erc20_transfer_transaction ON erc20_transfer (transaction);
CREATE INDEX IF NOT EXISTS erc20_transfer_from ON erc20_transfer ("from");
CREATE INDEX IF NOT EXISTS erc20_transfer_to ON erc20_transfer ("to");
CREATE INDEX IF NOT EXISTS erc20_transfer_contract ON erc20_transfer (contract);

CREATE TABLE IF NOT EXISTS "transaction" (
    vid BIGSERIAL PRIMARY KEY, id TEXT NOT NULL UNIQUE,
    timestamp NUMERIC NOT NULL, block_number BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS transaction_block ON "transaction" (block_number);

-- A read view reconstructs handler values from native columns; no JSON is stored.
CREATE OR REPLACE VIEW erc20_state AS
SELECT 'Account'::TEXT AS entity_type, id AS entity_id,
    jsonb_build_object('id', id, 'asERC20', as_erc20) AS data,
    lower(block_range) AS block_number FROM account WHERE upper_inf(block_range)
UNION ALL SELECT 'ERC20Contract', id,
    jsonb_build_object('id', id, 'asAccount', as_account, 'name', name, 'symbol', symbol,
        'decimals', decimals, 'totalSupply', total_supply), block_number FROM erc20_contract
UNION ALL SELECT 'ERC20Balance', id,
    jsonb_build_object('id', id, 'contract', contract, 'account', account,
        'value', value::TEXT, 'valueExact', value_exact::TEXT), lower(block_range)
    FROM erc20_balance WHERE upper_inf(block_range)
UNION ALL SELECT 'ERC20Transfer', id,
    jsonb_build_object('id', id, 'emitter', emitter, 'transaction', transaction,
        'timestamp', timestamp::TEXT, 'contract', contract, 'from', "from", 'fromBalance', from_balance,
        'to', "to", 'toBalance', to_balance, 'value', value::TEXT, 'valueExact', value_exact::TEXT),
    block_number FROM erc20_transfer
UNION ALL SELECT 'Transaction', id,
    jsonb_build_object('id', id, 'timestamp', timestamp::TEXT, 'blockNumber', block_number::TEXT),
    block_number FROM "transaction";

-- Applies final ERC20 entity changes at height within the caller's transaction.
-- Mutable rows retain block ranges; immutable entities cannot change or be deleted.
CREATE OR REPLACE FUNCTION erc20_apply(height BIGINT, changes JSONB) RETURNS VOID
LANGUAGE plpgsql SET search_path FROM CURRENT AS $apply$
DECLARE change JSONB; data JSONB; previous JSONB;
BEGIN
    FOR change IN SELECT value FROM jsonb_array_elements(changes) LOOP
        data := NULLIF(change->'data', 'null'::JSONB);
        IF change->>'kind' IN ('ERC20Contract', 'ERC20Transfer', 'Transaction') THEN
            SELECT state.data INTO previous FROM erc20_state state
                WHERE entity_type = (change->>'kind') AND entity_id = (change->>'id');
            IF data IS NULL OR (previous IS NOT NULL AND previous <> data)
            THEN RAISE EXCEPTION 'immutable ERC20 entity cannot be changed or deleted'; END IF;
            IF previous IS NOT NULL THEN CONTINUE; END IF;
        END IF;
        CASE change->>'kind'
        WHEN 'Account' THEN
            DELETE FROM account WHERE id = (change->>'id') AND lower(block_range) = height;
            UPDATE account SET block_range = int8range(lower(block_range), height, '[)')
                WHERE id = (change->>'id') AND upper_inf(block_range);
            IF data IS NOT NULL THEN
                INSERT INTO account (block_range, id, as_erc20)
                VALUES (int8range(height, NULL, '[)'), data->>'id', data->>'asERC20');
            END IF;
        WHEN 'ERC20Balance' THEN
            DELETE FROM erc20_balance WHERE id = (change->>'id') AND lower(block_range) = height;
            UPDATE erc20_balance SET block_range = int8range(lower(block_range), height, '[)')
                WHERE id = (change->>'id') AND upper_inf(block_range);
            IF data IS NOT NULL THEN
                INSERT INTO erc20_balance (block_range, id, contract, account, value, value_exact)
                VALUES (int8range(height, NULL, '[)'), data->>'id', data->>'contract', data->>'account',
                    (data->>'value')::NUMERIC, (data->>'valueExact')::NUMERIC);
            END IF;
        WHEN 'ERC20Contract' THEN
            INSERT INTO erc20_contract (block_number, id, as_account, name, symbol, decimals, total_supply)
            VALUES (height, data->>'id', data->>'asAccount', data->>'name', data->>'symbol',
                (data->>'decimals')::INTEGER, data->>'totalSupply');
        WHEN 'ERC20Transfer' THEN
            INSERT INTO erc20_transfer (block_number, id, emitter, transaction, timestamp, contract,
                "from", from_balance, "to", to_balance, value, value_exact)
            VALUES (height, data->>'id', data->>'emitter', data->>'transaction', (data->>'timestamp')::NUMERIC,
                data->>'contract', data->>'from', data->>'fromBalance', data->>'to', data->>'toBalance',
                (data->>'value')::NUMERIC, (data->>'valueExact')::NUMERIC);
        WHEN 'Transaction' THEN
            INSERT INTO "transaction" (id, timestamp, block_number)
            VALUES (data->>'id', (data->>'timestamp')::NUMERIC, (data->>'blockNumber')::BIGINT);
        ELSE RAISE EXCEPTION 'unknown ERC20 entity';
        END CASE;
    END LOOP;
END; $apply$;

-- Removes versions and immutable rows from height onward, then reopens prior ranges.
CREATE OR REPLACE FUNCTION erc20_revert(height BIGINT) RETURNS VOID
LANGUAGE plpgsql SET search_path FROM CURRENT AS $revert$
BEGIN
    DELETE FROM account WHERE lower(block_range) >= height;
    UPDATE account SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
    DELETE FROM erc20_balance WHERE lower(block_range) >= height;
    UPDATE erc20_balance SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= height;
    DELETE FROM erc20_contract WHERE block_number >= height;
    DELETE FROM erc20_transfer WHERE block_number >= height;
    DELETE FROM "transaction" WHERE block_number >= height;
END; $revert$;
