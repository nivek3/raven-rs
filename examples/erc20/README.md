# ERC-20 Example

Indexes one token's Transfer logs with `RpcLogCrawler`. Balances, history and
canonical progress commit and roll back together in application-owned tables.

## Indexed entities

| Entity | ID | Native table / history |
| --- | --- | --- |
| `Account` | Lowercase address | `account` / block range |
| `ERC20Contract` | Lowercase token address | `erc20_contract` / creation block |
| `ERC20Balance` | `token/account` or `token/totalSupply` | `erc20_balance` / block range |
| `ERC20Transfer` | `blockNumber-logIndex` | `erc20_transfer` / creation block |
| `Transaction` | Lowercase transaction hash | `transaction` / creation block |

`valueExact` is a signed arbitrary-precision integer; `value` applies token decimals
with 34 significant digits and no floating point. Addresses/hashes are lowercase
hexadecimal. Handler JSON uses decimal strings; SQL uses `NUMERIC` amounts,
`INTEGER` decimals and `BIGINT` positions.

Zero balances, zero-value transfers and self-transfers remain indexed.
Mint/burn endpoints are null; the zero address does not become an Account.
Supply is a balance with `account: null`; `total_supply` references its ID.
Only Transfer events are indexed.

## Supported tokens and indexing start

Every balance change must emit a standard Transfer. Rebasing/reflection tokens
are unsupported. Start at deployment or before the first Transfer; starting late
accumulates from zero and may produce negative balances or supply.

RPC must retain indexed blocks, the start parent and historical state for metadata.
At the first Transfer, metadata is read at its block hash: reverted/undecodable
calls yield null name/symbol and decimals 18; transport or historical-state errors
stop processing. Exact-hash timestamps are fetched once per indexed transaction.

## Configuration

CLI flags override environment variables; `.env` loads from the working directory.
Use environment configuration for database credentials.

| Flag | Environment | Required / default |
| --- | --- | --- |
| `--rpc-url` | `RAVEN_RPC_URL` | Required HTTP(S) RPC |
| `--database-url` | `RAVEN_DATABASE_URL` | Required PostgreSQL URL |
| `--schema` | `RAVEN_SCHEMA` | Required dedicated schema |
| `--network-name` | `RAVEN_NETWORK_NAME` | Required network name |
| `--token` | `RAVEN_TOKEN` | Required token address |
| `--start-block` | `RAVEN_START_BLOCK` | Required complete-history start |
| `--confirmations` | `RAVEN_CONFIRMATIONS` | `12` |
| `--max-block-range` | `RAVEN_MAX_BLOCK_RANGE` | `1000` |
| `--block-concurrency` | `RAVEN_BLOCK_CONCURRENCY` | `10` |
| `--metrics-listen-addr` | `RAVEN_METRICS_LISTEN_ADDR` | Disabled |

From the workspace root, replace placeholders and define `LOCAL_DATABASE_URL`:

```sh
export RAVEN_RPC_URL="<rpc-url>"
export RAVEN_DATABASE_URL="$LOCAL_DATABASE_URL"
export RAVEN_SCHEMA=erc20_holders
export RAVEN_NETWORK_NAME="<network-name>"
export RAVEN_TOKEN="<token-address>"
export RAVEN_START_BLOCK="<deployment-block>"
cargo run -p raven-example-erc20 --locked --offline
```

Inspect options with `--help`. To enable [Prometheus](../../docs/monitoring.md), add
`--metrics-listen-addr 127.0.0.1:9464`. Use a fresh schema when changing the token
or start; Raven detects changed chain IDs/network names, not token changes.

## Relational storage

[Entities](src/entities.rs), [schema](src/schema.sql) and [Storage](src/storage.rs)
define the mapping. `erc20_state` reconstructs handler JSON from native tables;
`erc20_apply` / `erc20_revert` run inside Raven's transaction.

Mutable rows use `[start, end)` ranges; immutable rows reject updates/deletes.
Rollback removes versions created at that block and reopens predecessors.
`vid` is an internal row identity. See the [storage contract](../../docs/storage.md)
for writer connection requirements and query consistency.

## Local Anvil smoke test

Use [local acceptance](../README.md#local-acceptance) for automated deployment,
mint/burn/transfers, indexing and history checks. The manual token fixture lives
in [anvil](anvil); it has 18 decimals. For manual runs, use confirmations `0` and
a fresh schema for each new Anvil chain.

## Run the rollback example

Configure a dedicated `RAVEN_DATABASE_URL`, then run:

```sh
cargo run -p raven-testkit --locked --offline -- erc20-rollback
```

It indexes branch A, reverts only Anvil, creates branch B at the same height,
and restarts the indexer. Recovery must remove orphaned transfers/versions and
match a clean replay. Raw-unit balances are:

| Stage | Account 0 | Account 1 | Account 2 | Supply |
| --- | ---: | ---: | ---: | ---: |
| Initial mint | 1000 | absent | absent | 1000 |
| A: transfer 300 to Account 1 | 700 | 300 | absent | 1000 |
| B: transfer 120 to Account 2 | 880 | absent | 120 | 1000 |

Schemas remain for inspection. See [testkit](../testkit) for prerequisites,
`rollback-report.json` and logs. Business rollback lives in SQL; no separate
Transfer rollback handler is needed.

## Query balances and supply

Use your own SQL connection; replace `erc20_holders` with your schema:

```sql
SELECT id, account, value_exact, value FROM erc20_holders.erc20_balance
WHERE upper_inf(block_range) AND account IS NOT NULL ORDER BY value_exact DESC, id;

SELECT value_exact, value FROM erc20_holders.erc20_balance
WHERE upper_inf(block_range) AND account IS NULL;

SELECT id, "from", "to", value_exact, "transaction" FROM erc20_holders.erc20_transfer
ORDER BY block_number, split_part(id, '-', 2)::numeric;
```

For positive holders, add `value_exact > 0`. Rust callers can use
`balances_at` with a supplied connection, optional height and optional account.

## Inspect an indexed height

At a committed canonical height, mutable tables use range containment and
immutable tables use creation positions:

```sql
SELECT id, account, value_exact FROM erc20_holders.erc20_balance
WHERE block_range @> 14::bigint;
SELECT id, "from", "to", value_exact FROM erc20_holders.erc20_transfer
WHERE block_number <= 14;
```

Reorg can invalidate a chosen height/hash. Related reads should share a snapshot.

## Tests

```sh
cargo test -p raven-example-erc20 --lib --locked --offline
```
