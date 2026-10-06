# Local Testkit

Local Anvil/PostgreSQL acceptance for both examples. Run from the workspace root
with Rust/Cargo, Foundry (`forge`, `cast`, `anvil`), `psql`, cached Rust dependencies
and Solidity 0.8.30 available. Builds run offline; no mainnet RPC or fork is used.

## Configuration

Set `RAVEN_DATABASE_URL` in the workspace `.env` or environment to an existing,
dedicated test database. Do not execute `.env` with `source` or commit credentials.
Create the database once if needed:

```sh
createdb -h <host> -p <port> -U <user> --password <database>
```

The testkit preserves the database and previous runs. Full verification retains
four mapping schemas: `erc20`, `uniswap_v3`, `erc20_clean`, `uniswap_v3_clean`,
each with a unique run prefix. Database regression tests create/drop their own
temporary schemas.

## Running the Testkit

```sh
RAVEN_DATABASE_URL="$LOCAL_DATABASE_URL" \
  cargo run -p raven-testkit --locked --offline -- verify
```

Use the appropriate subcommand:

| Command | Coverage |
| --- | --- |
| `verify` | Workspace tests/builds, ignored PostgreSQL/Uniswap tests, both indexers, history, restart, reorg and clean replay |
| `erc20-rollback` | ERC20 fixture/indexer build and automatic same-height rollback; skips workspace tests |
| `official-v3-smoke` | Official V3 contracts, day/hour swaps and same-height fork; no indexer, PostgreSQL or workspace tests |

All commands start owned Anvil processes; the first two also start indexers.
Managed processes stop afterward and mapping schemas remain. Only report
`status: "passed"` confirms success.

## Reports and Logs

The printed temporary directory contains process logs and a JSON report:

| Command | Report | Schema fields |
| --- | --- | --- |
| `verify` | `report.json` | `schemas.erc20`, `schemas.uniswap_v3`, `schemas.erc20_clean`, `schemas.uniswap_v3_clean` |
| `erc20-rollback` | `rollback-report.json` | `schema`, `clean_schema` |
| `official-v3-smoke` | `official-v3-smoke-report.json` | None |

Database reports identify the actual `database`. Rollback includes balance
`stages` and canonical/orphan hashes; the smoke report records contract addresses
and receipts. Inspect a report with:

```sh
python3 -m json.tool /tmp/raven-indexing-.../report.json
```

## Querying Retained Schemas

Read the schema name from the report and connect to that report's database:

```sh
SCHEMA="<schema-from-report>"
psql -h <host> -p <port> -U <user> -d <database> --password -v schema="$SCHEMA"
```

Inside `psql`, query ERC20 state and progress:

```sql
SET search_path TO :"schema", pg_catalog;
SELECT account, value_exact, block_range FROM erc20_balance
WHERE upper_inf(block_range) ORDER BY account NULLS LAST;
SELECT id, "from", "to", value_exact FROM erc20_transfer ORDER BY block_number, id;
SELECT number, hash, status FROM blocks ORDER BY number, status;
SELECT chain_id, start_block, latest_block_number, latest_block_hash FROM networks;
```

Reconnect with the Uniswap schema for pools, positions and swaps:

```sql
SET search_path TO :"schema", pg_catalog;
SELECT id, liquidity, total_value_locked_usd FROM pool WHERE upper_inf(block_range);
SELECT id, owner, pool, liquidity FROM position WHERE upper_inf(block_range);
SELECT id, pool, amount0, amount1 FROM swap ORDER BY block_number, log_index;
```

ERC20 supply has `account IS NULL`. Mutable rows use half-open `INT8RANGE` values:
`upper_inf(block_range)` selects current state; `block_range @> <height>::bigint`
selects history. Immutable rows use `block_number`. Block status is `1` canonical,
`0` orphaned. See the example READMEs for more queries.
