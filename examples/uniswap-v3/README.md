# Uniswap V3 example

Indexes a Factory, its pools and NFT positions into application-owned PostgreSQL
history. The mapping owns pricing policy, SQL and rollback.

## Stored model

| Entities | History |
| --- | --- |
| Factory, Bundle, Token, Pool, Tick | `INT8RANGE` |
| Position, PositionSnapshot | `INT8RANGE` |
| UniswapDayData, PoolDayData/HourData, TokenDayData/HourData, TickDayData/HourData | `INT8RANGE` |
| Transaction, Mint, Burn, Swap, Collect, Flash | Creation `block_number` |

See [entities](src/entities.rs) and [schema](src/schema.sql) for all columns.
`pool_state` reconstructs handler JSON; joins resolve reverse relations.
Addresses/hashes are lowercase text, amounts use `NUMERIC`, positions use
`BIGINT`, and whitelist pools use `TEXT[]`. Raw integers remain exact;
`BigDecimal` calculations round each operation to 34 significant digits.

## Event processing and calculations

Two crawlers acquire pool event signatures and address-bound position-manager
signatures, then merge by block/header and global log index. This avoids wildcard
ERC20 Transfer acquisition. Streaming and recovery use the same rules.

PoolCreated initializes metadata and membership; later logs in that block can
use the staged pool. Unknown pool emitters are ignored before ABI decoding.
Metadata, origin, gas price, timestamps and fee growth use exact block-hash reads.
RPC failures stop processing; reverted metadata uses string/bytes32 fallbacks,
`unknown`, zero supply and unavailable-decimal behavior. One block payload is
cached by hash.

The mapping maintains liquidity, reserves, prices, volumes, fees and day/hour
aggregates. These field-specific rules also apply:

<details>
<summary>Mapping rules and limitations</summary>

- `txCount` counts relevant events. `Transaction.gasUsed` is `0`; gas price comes from the transaction.
- Mint/Burn/Swap IDs are `<transaction hash>#<incremented pool txCount>`; Tick IDs are `<pool>#<tick>`.
- Burn removes liquidity/reserves/TVL. Flash only updates fee growth, without entities or counters; Pool Collect is unhandled. Collect, Flash and TickHourData tables remain empty.
- Mint/Burn and bounded Swap crossings read tick fee growth and update TickDayData, which copies `volumeToken0` into both volume fields.
- Position IDs are decimal NFT IDs; snapshots use `<NFT ID>#<block number>`. Same-block events share a snapshot. NFT Collect uses amount0/token0 decimals for both fee fields; reverting `positions()` reads for missing positions are ignored.
- Token `poolCount`, factory owner and some untracked TVL fields retain initialized values. Save ordering can expose prior Pool/Factory values to intervals. Token interval untracked volume receives tracked volume; protocol day untracked volume stays zero.
- Token TVL groups Mint/Burn arithmetic as `balance * (derivedETH * ethPriceUSD)` and Swap as `(balance * derivedETH) * ethPriceUSD`; rounding makes the difference observable.
- Two hardcoded Swap pricing exclusions, skipped block/pool rules and static metadata remain application policy.

</details>

## Configuration

CLI flags override environment variables; `.env` loads from the working directory.

| Flag | Environment | Required / default |
| --- | --- | --- |
| `--rpc-url` | `RAVEN_RPC_URL` | Required block-hash calls/logs |
| `--database-url` | `RAVEN_DATABASE_URL` | Required PostgreSQL URL |
| `--schema` | `RAVEN_SCHEMA` | Required dedicated schema |
| `--network-name` | `RAVEN_NETWORK_NAME` | Required network name |
| `--factory` | `RAVEN_FACTORY` | Required Factory address |
| `--start-block` | `RAVEN_START_BLOCK` | Required complete-history start |
| `--position-manager` | `RAVEN_POSITION_MANAGER` | `0xc36442b4a4522e871399cd717abdd847ab11fe88` |
| `--chain-config` | `RAVEN_CHAIN_CONFIG` | Ethereum policy |
| `--confirmations` | `RAVEN_CONFIRMATIONS` | `12` |
| `--metrics-listen-addr` | `RAVEN_METRICS_LISTEN_ADDR` | Disabled |

After setting required values, run from the workspace root:

```sh
cargo run -p raven-example-uniswap-v3 --locked --offline
```

Use `--help` for options; add `--metrics-listen-addr 127.0.0.1:9465` for
[Prometheus](../../docs/monitoring.md). SIGINT/SIGTERM stops indexing; restart
resumes persisted progress.

Start at Factory deployment or earlier; missing PoolCreated events means partial
state. Comparisons require the same pools, policy and indexed height.
[ethereum.json](config/ethereum.json) defines reference assets, pools, exclusions
and thresholds (minimum reference liquidity: 60 ETH). Other chains need their own
`--chain-config` and position manager; network name does not select pricing policy.

## History and rollback

Mutable versions use `[start, end)`; rollback deletes new versions and reopens
predecessors. Immutable events are deleted by creation height. Pool membership
and whitelists are versioned, so orphaned discovery disappears on recovery.
See [Storage](../../docs/storage.md) for transaction and writer requirements.

```sql
SELECT id, liquidity, total_value_locked_usd FROM uniswap_v3.pool
WHERE upper_inf(block_range);
SELECT p.id, t.symbol, p.liquidity FROM uniswap_v3.pool p
JOIN uniswap_v3.token t ON t.id = p.token0
WHERE p.block_range @> 12370000::bigint AND t.block_range @> 12370000::bigint;
SELECT id, owner, pool, liquidity FROM uniswap_v3.position WHERE upper_inf(block_range);
SELECT id, amount0, amount1, amount_usd FROM uniswap_v3.swap
WHERE block_number <= 12370000 ORDER BY block_number, log_index;
```

Replace `uniswap_v3` with your schema and use one snapshot for related reads.

## Verification

Use [testkit](../testkit) with a dedicated test database. `verify` tests all 20
native mappings, historical snapshots, restart, same-height rollback and equality
with clean replay on local Anvil. It deploys official V3 Factory/position manager/
router artifacts pinned to core 1.0.1 and periphery 1.4.4, with SHA256 checks.
Only test ERC20/WETH assets are fixtures.

Contract calls/receipts verify metadata, liquidity, fee growth, positions and
event amounts; independent decimal calculations check pricing. Swaps across
day/hour boundaries test aggregates. The fork creates an orphan pool and changes
NFT state. Flash and deliberately reverting `positions()` are not exercised by
the contract sequence; malformed inputs/RPC errors are covered by Rust tests.

`official-v3-smoke` checks only contract deployment/lifecycle, day/hour swaps and
the fork, without an indexer or PostgreSQL. Neither mode uses mainnet or proves
production throughput. See testkit reports for completion and retained schemas.
