# Uniswap V3 example

This example indexes a Uniswap V3 Factory, the pools it creates and an NFT position
manager into native PostgreSQL tables. The application owns its entities, pricing
policy, SQL tables, historical versions and rollback logic. Raven's core store
coordinates their writes with canonical blocks and indexing progress.

## Stored model

| Entities | Native tables | Storage |
| --- | --- | --- |
| Factory, Bundle, Token, Pool, Tick | `factory`, `bundle`, `token`, `pool`, `tick` | `INT8RANGE` versions |
| Position, PositionSnapshot | `position`, `position_snapshot` | `INT8RANGE` versions |
| Transaction, Mint, Burn, Swap, Collect, Flash | `transaction`, `mint`, `burn`, `swap`, `collect`, `flash` | Immutable; `block_number BIGINT` |
| UniswapDayData, PoolDayData, PoolHourData, TokenDayData, TokenHourData | `uniswap_day_data`, `pool_day_data`, `pool_hour_data`, `token_day_data`, `token_hour_data` | `INT8RANGE` versions |
| TickDayData, TickHourData | `tick_day_data`, `tick_hour_data` | `INT8RANGE` versions |

Rust serde definitions live in `src/entities.rs`. The native field columns,
indexes, computed read view and apply/revert functions live in `src/schema.sql`.
Handlers use these concrete structs directly. Arbitrary-precision numeric fields
use the library's `bigdecimal::BigDecimal`, with native string serialization at
the `EntityStore` boundary. Chain integers remain exact; price and amount
calculations explicitly round each arithmetic step to 34 significant digits.
Handlers serialize these typed values at the `EntityStore` boundary. SQL filters
and joins resolve reverse relations, and `Token.whitelistPools` is a native
`TEXT[]` because the mapping updates that list. Addresses, hashes and references
use lowercase hexadecimal text. Amounts and arbitrary-precision integers use
`NUMERIC`; block positions use `BIGINT`. `Transaction.blockNumber` is its only
creation-position column. The schema relies on application validation rather than
explicit `CHECK` constraints.

`pool_state` is an ordinary non-materialized view. It reconstructs serde values
from native columns for handler reads.

## Event processing and calculations

Factory `PoolCreated` initializes token metadata and pool state. The application
selects candidate signatures, checks persisted/staged Pool membership, then
decodes the ABI. Pools created earlier in the same block are immediately visible.
Unknown pool emitters are ignored before decoding, including malformed bodies.
Discovered pools are persisted in application state and become eligible for later
logs in the same block.
Candidate acquisition uses two application-owned crawlers: a wildcard address
filter for PoolCreated/Initialize/Mint/Burn/Swap/Flash, and a filter bound to the
position manager for IncreaseLiquidity/DecreaseLiquidity/Collect/Transfer.
The example merges identical block headers and orders logs by global log index.
This avoids requesting every ERC20 Transfer on mainnet. Both streaming and
reorg lookups use the same acquisition rules.

Metadata, transaction origin, gas price and timestamp are read using the exact
block hash. RPC failures stop processing; reverted metadata calls use the
string/bytes32 fallback, `unknown`, zero supply and unavailable-decimal behavior.
The application caches only one immutable block payload by hash.

The handlers update active liquidity, tick gross/net liquidity, token reserves,
prices, tracked/untracked volumes, fee totals and day/hour OHLC aggregates.
Decimal price and amount arithmetic uses `BigDecimal` and rounds each operation to
34 significant digits. Raw integers such as supply, liquidity, square-root prices,
ticks and counters retain full precision. `sqrtPriceX96` uses the exact squared
integer and token decimal scaling; no binary floating point is used.

The current application rules include:

- Factory/Pool/Token `txCount` counts the relevant events, not distinct hashes.
- Burn adjusts active/tick liquidity and removes token reserves/TVL.
- `Transaction.gasUsed` is currently set to `0`; gas price comes from the transaction.
- Mint/Burn/Swap IDs are `<transaction hash>#<pool txCount>` after increment;
  Tick IDs are `<pool>#<tick>`.
- Flash updates pool fee-growth variables. It creates no Flash entity or
  Transaction and does not increment counters. Pool Collect is not handled.
- The index never writes Collect, Flash or TickHourData entities, although
  its schema declares them; their tables remain empty during ordinary indexing.
- Mint/Burn and bounded Swap crossings read tick fee-growth variables at the exact
  block hash and update TickDayData. TickDayData copies `volumeToken0` into both
  volume fields.
- Position IDs are decimal NFT IDs; snapshots use `<NFT ID>#<block number>`.
  Multiple NFT events in one block update one snapshot. NFT Collect increments
  both collected-fee fields using amount0/token0 decimals. Missing positions whose
  `positions()` call reverts are ignored. The configured skipped block and pool
  are preserved.
- Token `poolCount`, factory owner and several untracked TVL fields retain their
  initialized values until application logic updates them.
- Interval reads follow the handler save ordering. Some snapshots observe
  prior Pool/Factory values. Token interval `untrackedVolumeUSD` receives tracked
  volume, and UniswapDayData `volumeUSDUntracked` remains initialized to zero.
- Mint/Burn calculate token TVL as `balance * (derivedETH * ethPriceUSD)`;
  Swap calculates `(balance * derivedETH) * ethPriceUSD`. Per-operation rounding
  makes this grouping observable, so the handlers retain each calculation order.
- The two hardcoded Swap pricing exclusions are preserved. Configured skipped
  pools and static token metadata are application policy.

These rules define the current indexed state; field names alone do not imply a
different calculation.

## Configuration

CLI flags override matching environment variables. `.env` is loaded from the
working directory; provide connection settings through your environment.

| Flag | Environment | Meaning |
| --- | --- | --- |
| `--rpc-url` | `RAVEN_RPC_URL` | HTTP(S) RPC with block-hash calls and logs |
| `--database-url` | `RAVEN_DATABASE_URL` | PostgreSQL connection, supplied by the application |
| `--schema` | `RAVEN_SCHEMA` | Dedicated indexing schema |
| `--network-name` | `RAVEN_NETWORK_NAME` | Configured name, such as `ethereum-mainnet` |
| `--factory` | `RAVEN_FACTORY` | Factory address |
| `--position-manager` | `RAVEN_POSITION_MANAGER` | NFT manager; defaults to the Ethereum mainnet address |
| `--start-block` | `RAVEN_START_BLOCK` | Factory deployment block or an earlier complete-history start |
| `--chain-config` | `RAVEN_CHAIN_CONFIG` | Optional application policy JSON; defaults to the Ethereum policy |
| `--confirmations` | `RAVEN_CONFIRMATIONS` | Confirmation delay; default 12 |

`config/ethereum.json` contains the Ethereum reference token, stable-price pool,
liquidity threshold, whitelist, stablecoins, skipped pools and static token
definitions. Other chains must supply the corresponding policy through
`--chain-config`. `network_name` is stored as network metadata; it does not select
pricing constants. The Anvil verifier supplies its own fixture policy.
The Ethereum minimum reference liquidity is 60 ETH.

After setting the environment:

```sh
cargo run -p raven-example-uniswap-v3 --locked --offline
```

For Ethereum, set `--factory` and `--start-block` for the deployment you intend
to index. Starting after PoolCreated events loses pool discovery and produces
partial state. Price and protocol aggregates also require all relevant pools and
the same policy/indexed height for comparison.
Use a direct PostgreSQL connection or session pooling for the writer. Queries
use their own SQLx pool or SQL client. Ctrl-C/SIGTERM stops indexing and releases
the writer session. Reopening resumes the persisted indexing start/progress.
The position manager is `0xc36442b4a4522e871399cd717abdd847ab11fe88`.

## History and rollback

Mutable rows retain half-open `[start, end)` block ranges. Updates close the
previous version; deletions close it without a successor. Reorg removes versions
created at the reverted height and reopens their predecessors. Immutable event
rows are deleted by creation height. Pool membership and whitelist arrays are
versioned too, so orphaned pool discovery disappears after rollback/restart.
Business versions, canonical block status and progress share Raven's transaction.

```sql
SELECT id, liquidity, token0_price, token1_price, total_value_locked_usd
FROM uniswap_v3.pool WHERE upper_inf(block_range);

SELECT p.id, t.symbol, p.liquidity
FROM uniswap_v3.pool p JOIN uniswap_v3.token t ON t.id = p.token0
WHERE p.block_range @> 12370000::bigint AND t.block_range @> 12370000::bigint;

SELECT id, transaction, amount0, amount1, amount_usd
FROM uniswap_v3.swap WHERE block_number <= 12370000 ORDER BY block_number, log_index;

SELECT id, owner, pool, liquidity, deposited_token0, collected_fees_token0
FROM uniswap_v3.position WHERE upper_inf(block_range);

SELECT id, owner, liquidity, block_number
FROM uniswap_v3.position_snapshot
WHERE position = '7' AND upper_inf(block_range) ORDER BY block_number;
```

Related queries can share an application-owned repeatable-read, read-only SQLx
transaction, as shown in the [storage guide](../../crates/raven-postgres/README.md).

## Verification

Set `RAVEN_DATABASE_URL` in the workspace `.env` to a dedicated test database,
then run:

```sh
cargo run -p raven-testkit --locked --offline -- uniswap
cargo run -p raven-testkit --locked --offline -- verify
```

Both entrypoints run the same workspace acceptance. It builds/tests Rust, checks
all 20 native mappings, and deploys the official V3 Factory, position manager and
swap router on its own Anvil chain. The Factory creates real V3 pools. Liquidity,
swaps, NFT collection and transfers execute through the official contracts.
Only the test ERC20/WETH assets are local fixture implementations.

The official creation artifacts are pinned to `@uniswap/v3-core` 1.0.1 and
`@uniswap/v3-periphery` 1.4.4 in `anvil/official-artifacts.lock.json`. Their SHA256
digests are checked before deployment. The verifier also runs ERC20/backend
regression tests, checks SQL history, restart and reorg, and compares all native
table versions with fresh canonical replay. Individual checks are recorded in
the generated report.

The V3 checks compare token metadata, pool fee/price/tick/liquidity and fee growth,
NFT owner/liquidity, and event amounts with contract calls and canonical receipts.
An independent 34-digit decimal calculation checks stored prices. A real swap
across a day/hour boundary checks new aggregate buckets and OHLC while historical
SQL snapshots remain stable. The orphan branch creates a second pool with a
different fee, then rollback must remove that pool and restore NFT state.
The local contract sequence does not exercise Flash or deliberately reverting
`positions()` calls.

Each run creates and retains dynamic schemas in the configured database, including
schemas such as `raven_<runid>_erc20` and `raven_<runid>_uniswap_v3`. The generated
report records the actual database and schema names for inspection. These commands
need Cargo/Rust, psql, Foundry and cached Rust/Solidity dependencies; builds use
offline mode. Owned Anvil/indexer processes stop at the end. Only a report with
`status: "passed"` confirms successful completion.

For a quick deployment and contract lifecycle check without workspace tests or PostgreSQL:

```sh
cargo run -p raven-testkit --locked --offline -- uniswap --official-v3-smoke
```

This mode also exercises a swap across a day/hour boundary and an Anvil
same-height reorg that removes a second pool. It checks contract state and
receipts; it does not run the indexer or check persisted SQL rollback.

Anvil runs locally without a mainnet fork. These checks use no mainnet RPC and
require no synchronized mainnet node. They validate local contract/indexer
behavior and do not establish production throughput. Artificial malformed logs
and RPC error behavior remain covered by the Rust regression tests rather than
injected into official pool contracts.
