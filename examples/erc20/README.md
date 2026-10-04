# ERC-20 Example

This example scans one token's canonical Transfer events with
`rpc-log-crawler-datasource` and stores token, account, balance, transaction and
Transfer entities in separate typed PostgreSQL tables. Entity fields, IDs and
balance calculations follow the ERC-20 Transfer model. Entity versions, transfer
history and indexing progress commit and roll back together.

## Indexed entities

This example indexes `Transfer` events only. Approval, ownership, pause and role
events are outside its scope.

| Entity | ID | Stored result |
| --- | --- | --- |
| `Account` | Lowercase address | `id`, optional `asERC20` |
| `ERC20Contract` | Lowercase token address | `asAccount`, `name`, `symbol`, `decimals`, `totalSupply` |
| `ERC20Balance` | `token/account` | Account balance: `contract`, `account`, `value`, `valueExact` |
| `ERC20Balance` | `token/totalSupply` | Supply calculated from mint/burn events, with `account: null` |
| `ERC20Transfer` | `blockNumber-logIndex` | Transfer amount, timestamp, transaction and account/balance references |
| `Transaction` | Lowercase transaction hash | `timestamp`, `blockNumber` |

In the handler's JSON state, addresses and hashes are lowercase hexadecimal
strings, and BigInt/BigDecimal fields are decimal strings, including transaction
timestamps and block numbers. The relational tables use the native types below.
`valueExact` retains the signed, arbitrary-precision integer. `value` applies token
decimals with `BigDecimal`, rounded to 34 significant digits without floating
point.

Zero balances remain indexed. Self-transfers and zero-value transfers retain their
records. Mint transfers have null `from`/`fromBalance`; burn transfers have null
`to`/`toBalance`. The zero address does not become an Account. Reverse relations
are resolved by filtering and joining relational columns.

## Relational storage

The application defines plain serde entities in `src/entities.rs` and native
relations, read mapping and version rules in `src/schema.sql`. Its `Storage`
implements `PostgresStorage` to read entities, apply a block and revert a block
using the connection supplied by Raven. See the
[storage contract](../../crates/raven-postgres/README.md).

| Entity | PostgreSQL table | Version columns |
| --- | --- | --- |
| `Account` | `account` | `vid`, `block_range` (`int8range`) |
| `ERC20Contract` | `erc20_contract` | `vid`, `block_number` (`bigint`) |
| `ERC20Balance` | `erc20_balance` | `vid`, `block_range` (`int8range`) |
| `ERC20Transfer` | `erc20_transfer` | `vid`, `block_number` (`bigint`) |
| `Transaction` | `transaction` | `vid`, `block_number` (`bigint`) |

Addresses/hashes and their references are lowercase `0x` hexadecimal `text`.
Amounts use native `numeric`; token decimals use `integer`. Transaction reuses
its business `block_number` as the creation position. Block positions and version
ranges use signed 64-bit integers. No explicit SQL `CHECK` constraints are added.
`total_supply` is a balance ID, not a supply amount.

Mutable entities retain half-open `[start, end)` ranges. `upper_inf(block_range)`
selects current rows; `block_range @> 14::bigint` selects state at block 14.
Immutable entities use `block_number <= 14`. Immutable updates/deletes fail the
application's block write.

All business state is read from these native tables. The non-materialized
`erc20_state` view reconstructs serde field names from native columns for handler
reads. The engine stages within-block changes in memory.

`erc20_apply` and `erc20_revert` are application-owned SQL functions called
explicitly in Raven's write transaction. Writes and progress commit together;
SQL failures roll back both. Reorg deletes new native versions and reopens their
predecessors. Independent SQL queries use application-owned pools and
transactions.

The first indexed Transfer initializes name, symbol and decimals with historical
`eth_call` requests at that event's block hash. Reverted or undecodable metadata
returns null name/symbol and default decimals 18. RPC errors
and unavailable historical state stop processing rather than silently setting
those defaults. The block timestamp is fetched by exact block hash once per
indexed transaction; further transfers in that transaction reuse its stored
metadata.

## Supported tokens and indexing start

The token must express every balance change through standard Transfer events.
Rebasing and reflection balances cannot be reconstructed from these events alone.
For complete balances and supply, `RAVEN_START_BLOCK` must be the deployment block
or an earlier block before the token's first Transfer. Starting late accumulates
from zero and can produce negative account balances or
supply. Those are partial-history results, rather than a live balance snapshot.

The RPC endpoint must retain the parent block and all indexed blocks, plus contract
state at the first indexed Transfer for metadata calls.

## Configuration

Clap accepts every setting as a command-line option or a matching environment
variable. A `.env` file in the working directory is loaded automatically. Command-line
options take precedence, while environment variables are preferable for the database
URL because it can contain credentials.

| Option | Environment variable | Required | Meaning |
| --- | --- | --- | --- |
| `--rpc-url` | `RAVEN_RPC_URL` | yes | HTTP(S) Ethereum RPC endpoint |
| `--database-url` | `RAVEN_DATABASE_URL` | yes | PostgreSQL connection URL |
| `--schema` | `RAVEN_SCHEMA` | yes | Dedicated Raven schema, for example `erc20_holders` |
| `--network-name` | `RAVEN_NETWORK_NAME` | yes | Configured network name, for example `ethereum-mainnet` or `anvil` |
| `--token` | `RAVEN_TOKEN` | yes | ERC-20 contract address |
| `--start-block` | `RAVEN_START_BLOCK` | yes | Deployment block or earlier complete-history start block |
| `--confirmations` | `RAVEN_CONFIRMATIONS` | no | Processing delay, default `12` |
| `--max-block-range` | `RAVEN_MAX_BLOCK_RANGE` | no | Maximum `eth_getLogs` range, default `1000` |
| `--block-concurrency` | `RAVEN_BLOCK_CONCURRENCY` | no | Concurrent block requests, default `10` |

Environment example:

```bash
export RAVEN_RPC_URL=https://ethereum-rpc.publicnode.com
export RAVEN_DATABASE_URL="$LOCAL_DATABASE_URL"
export RAVEN_SCHEMA=erc20_holders
export RAVEN_NETWORK_NAME=ethereum-mainnet
export RAVEN_TOKEN=0xYourTokenAddress
export RAVEN_START_BLOCK=12345678

cargo run -p raven-example-erc20 --locked --offline
```

Run `cargo run -p raven-example-erc20 -- --help` to inspect the equivalent
CLI options and defaults.

Treat the schema, token address and start block as one deployment configuration.
Raven does not detect a token-address change in an existing schema, so use a new schema
when changing the token or start block.
The `networks` table stores numeric `chain_id` and configured `network_name`
separately. A changed chain ID or network name is rejected on restart.

## Local Anvil smoke test

The `anvil` fixture provides deterministic Transfer logs without depending on a public
RPC endpoint. Start Anvil in one terminal:

```bash
anvil --host 127.0.0.1 --port 8545 --chain-id 31337
```

In another terminal, build and deploy the fixture with Anvil's first development
account as the initial holder:

```bash
ACCOUNT_0=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
ACCOUNT_1=0x70997970C51812dc3A010C7d01b50e0d17dc79C8
ACCOUNT_2=0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC

forge build --root examples/erc20/anvil
forge create --root examples/erc20/anvil \
  src/RavenTestToken.sol:RavenTestToken \
  --rpc-url http://127.0.0.1:8545 \
  --broadcast --unlocked --from "$ACCOUNT_0" \
  --constructor-args "$ACCOUNT_0" 1000
```

Set `TOKEN` to the deployed address printed by `forge create`, then create one normal
transfer, one forwarded transfer, one burn, one mint and one self-transfer:

```bash
TOKEN=0xDeployedContractAddress

cast send "$TOKEN" "transfer(address,uint256)" "$ACCOUNT_1" 300 \
  --rpc-url http://127.0.0.1:8545 --unlocked --from "$ACCOUNT_0"
cast send "$TOKEN" "transfer(address,uint256)" "$ACCOUNT_2" 120 \
  --rpc-url http://127.0.0.1:8545 --unlocked --from "$ACCOUNT_1"
cast send "$TOKEN" "burn(uint256)" 50 \
  --rpc-url http://127.0.0.1:8545 --unlocked --from "$ACCOUNT_2"
cast send "$TOKEN" "mint(address,uint256)" "$ACCOUNT_1" 25 \
  --rpc-url http://127.0.0.1:8545 --unlocked --from "$ACCOUNT_0"
cast send "$TOKEN" "transfer(address,uint256)" "$ACCOUNT_1" 10 \
  --rpc-url http://127.0.0.1:8545 --unlocked --from "$ACCOUNT_1"
```

Run Raven from the deployment block. Use a fresh schema for every new Anvil process,
because restarting Anvil creates a different chain:

```bash
export RAVEN_RPC_URL=http://127.0.0.1:8545
export RAVEN_DATABASE_URL="$LOCAL_DATABASE_URL"
export RAVEN_SCHEMA=erc20_holders_anvil
export RAVEN_NETWORK_NAME=anvil
export RAVEN_TOKEN="$TOKEN"
export RAVEN_START_BLOCK=1
export RAVEN_CONFIRMATIONS=0

cargo run -p raven-example-erc20 --locked --offline
```

Set `LOCAL_DATABASE_URL` to your local PostgreSQL connection settings before
running these commands. The fixture exposes name `Raven Test Token`, symbol
`RAVEN`, and decimals `18`.
After blocks 1 through 6 are committed, the expected results are:

| Account/result | `valueExact` | `value` |
| --- | --- | --- |
| `ACCOUNT_0` | `700` | `0.0000000000000007` |
| `ACCOUNT_1` | `205` | `0.000000000000000205` |
| `ACCOUNT_2` | `70` | `0.00000000000000007` |
| Total supply (`account: null`) | `975` | `0.000000000000000975` |

There are six ERC20Transfer records, including the mint, burn and self-transfer.
The Rust unit tests cover different token precisions, retained zero balances,
multiple logs in one transaction, partial-history signed balances, decimal
precision and metadata failure behavior.

For automated local deployment, PostgreSQL indexing, restart and fork/replay
checks, use the [workspace testkit](../README.md#local-acceptance).

## Run the rollback example

From the workspace root, set `RAVEN_DATABASE_URL` in `.env` to a dedicated test
database, then run:

```sh
cargo run -p raven-testkit --locked --offline -- erc20-rollback
```

This runnable example uses the actual ERC20 indexer, its Transfer handler and
application-owned PostgreSQL tables. It builds this Rust executable and the token
fixture offline, starts its own Anvil node, and creates dynamic schemas in the
configured database. It needs Cargo, Rust, Foundry (`forge`, `cast`, `anvil`),
`psql`, and the locally cached dependencies/Solidity compiler used by the smoke test.
The orchestration reuses the Rust testkit's deployment helpers.

The tool prints these three stages. Amounts are raw units (`valueExact`);
the fixture has 18 decimals. `absent` means that the account has no balance row.

| Stage | Account 0 | Account 1 | Account 2 | Total supply |
| --- | --- | --- | --- | --- |
| Before transfer | 1000 | absent | absent | 1000 |
| Branch A indexed: transfer 300 to Account 1 | 700 | 300 | absent | 1000 |
| Branch B after rollback: transfer 120 to Account 2 | 880 | absent | 120 | 1000 |

The flow in [erc20.rs](../testkit/src/erc20.rs) is:

1. Deploy the token and index its initial mint, then take an Anvil snapshot.
2. Transfer 300 to Account 1 and wait until Raven commits branch A to PostgreSQL.
3. Stop the indexer, revert only Anvil to the snapshot, and transfer 120 to
   Account 2, producing a different block hash at the same height.
4. Restart the indexer with the same database and schema. Engine detects the
   changed canonical hash, rolls back branch A, and applies branch B.
5. Check that the orphan transaction and native versions are removed, Account 1
   has disappeared, the deployment's historical state is preserved, and all five
   relational tables match a fresh canonical replay (excluding sequence values).

`evm_revert` changes the owned Anvil chain. The tool leaves Raven's stored state
intact so that restart demonstrates the Engine's automatic rollback. Your Rust
Transfer handler only applies events; it needs no separate rollback handler.

The generated schemas are retained for inspection. The report records the actual
database plus its rollback and clean-replay schema names, and the tool prints the
path to `rollback-report.json` and process logs. The restarted indexer's log
includes `Block reverted`.
Its owned Anvil and indexer processes stop when the demo finishes.

Inspect the result in the configured database. Block `status` is a `smallint`: `1`
means canonical and `0` means orphaned:

```sql
SELECT account, value_exact, block_range
FROM <schema_from_report>.erc20_balance
ORDER BY account NULLS LAST, lower(block_range);

SELECT number, hash, status FROM <schema_from_report>.blocks ORDER BY number, status;
```

## Query balances and supply

The example also exports `balances_at` and its `ERC20Balance` result type for
Rust callers. Pass a connection from your own SQLx transaction, an optional indexed block
number, and an optional account address. The query belongs to this example;
other applications supply their own SQL and result types.

All current account balances, including zero balances, can be listed in raw-unit order:

```sql
SELECT
    id,
    account AS account,
    value_exact AS "valueExact",
    value
FROM erc20_holders.erc20_balance
WHERE upper_inf(block_range) AND account IS NOT NULL
ORDER BY value_exact DESC, id;
```

For positive holders only, add `value_exact > 0`. The supply is
the ERC20Balance whose `account` is null:

```sql
SELECT b.id, b.value_exact, b.value
FROM erc20_holders.erc20_contract c
JOIN erc20_holders.erc20_balance b ON b.id = c.total_supply
WHERE upper_inf(b.block_range);
```

Transfer history is available through `erc20_transfer`. Mint/burn endpoints
remain SQL null rather than a zero-address account:

```sql
SELECT
    id,
    "from" AS "from",
    "to" AS "to",
    value_exact AS "valueExact",
    value,
    "transaction",
    timestamp
FROM erc20_holders.erc20_transfer
ORDER BY "block_number", split_part(id, '-', 2)::numeric;
```

## Inspect an indexed height

Read the Raven tables at a committed block (14 in this SQL example):

```sql
SELECT id AS id,
       as_erc20 AS "asERC20"
FROM erc20_holders.account WHERE block_range @> 14::bigint;

SELECT id AS id,
       as_account AS "asAccount",
       name, symbol, decimals, total_supply AS "totalSupply"
FROM erc20_holders.erc20_contract WHERE "block_number" <= 14;

SELECT id, contract AS contract,
       account AS account,
       value, value_exact AS "valueExact"
FROM erc20_holders.erc20_balance WHERE block_range @> 14::bigint;

SELECT id, emitter AS emitter,
       "transaction", timestamp, contract AS contract,
       "from" AS "from", from_balance AS "fromBalance",
       "to" AS "to", to_balance AS "toBalance",
       value, value_exact AS "valueExact"
FROM erc20_holders.erc20_transfer WHERE "block_number" <= 14;

SELECT id, timestamp, block_number AS "blockNumber"
FROM erc20_holders."transaction" WHERE "block_number" <= 14;
```

Derived lists are resolved through foreign IDs. Query a committed canonical height;
a reorg can invalidate a previously chosen height or hash. `vid` is an internal row
identity, while the typed tables retain canonical historical versions.

## Tests

Run the ERC-20 library tests:

```sh
cargo test -p raven-example-erc20 --lib --locked --offline
```
