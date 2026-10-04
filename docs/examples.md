# Examples

The examples are application mappings, not framework fixtures. Each owns its serde entities, native PostgreSQL schema, history rules, storage adapter, and configuration.

## ERC20

The [ERC20 example](https://github.com/nivek3/raven-rs/tree/main/examples/erc20) indexes a selected token's `Transfer` logs. It reads metadata at the first indexed transfer, tracks accounts, balances, total supply, transaction records, and transfer history. Its balances retain exact raw integers and decimal display values using the token's decimals.

It uses `RpcLogCrawler`, typed `LogParser<Transfer>`, `EntityStoreExt`, and a PostgreSQL version model based on ranges. It is the shortest end-to-end reference for building an entity mapping.

## Uniswap V3

The [Uniswap V3 example](https://github.com/nivek3/raven-rs/tree/main/examples/uniswap-v3) is a larger application mapping. It discovers pools from a Factory, maps pool events and NFT positions, and maintains pricing, liquidity, and time-bucketed aggregates. Its chain policy is application configuration, including reference assets and exclusions.

It demonstrates a custom source layered over block crawling, custom event parsing, mapping-specific contract calls, and a larger native SQL projection. Its README documents assumptions and correctness limits that belong to this specific mapping.

## Local testkit

`raven-testkit` is a local acceptance tool for the two examples. It starts local Anvil processes, uses a dedicated PostgreSQL database supplied through `RAVEN_DATABASE_URL`, and retains each run's generated schemas for inspection. It does not connect to mainnet or delete the database or schemas it creates.

The [testkit README](https://github.com/nivek3/raven-rs/blob/main/examples/testkit/README.md) describes prerequisites, commands, generated reports, and retained-schema SQL queries. It validates local behavior; it does not establish production RPC throughput or production deployment readiness.
