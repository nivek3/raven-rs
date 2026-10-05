# Raven

Raven is a Rust framework for indexing EVM chains into an application-owned projection. It acquires complete block batches, parses typed EVM updates, runs handlers in block order, and persists canonical progress with application writes.

Raven does not define an application's database model. An index owns its tables, SQL, version history, read queries, and rollback logic. The framework owns the canonical block pipeline and the transaction boundary that keeps that projection aligned with progress.

## What Raven provides

- Canonical block ingestion, resynchronization, rollback, and replay.
- Typed Alloy event and block parsers, plus ordered handlers.
- A block-local entity store, so later handlers see earlier changes in the same block before commit.
- Datasource and persistent chain-store contracts.
- A PostgreSQL store that commits application state, canonical block metadata, and progress together.
- Block-oriented and range-log-oriented RPC datasources.
- Canonical progress, processing, reorg and RPC metrics with an optional Prometheus exporter.

## What an application provides

- Contract selection, start block, parser and handler registration.
- Entity definitions and mapping-specific reads.
- PostgreSQL schema, current-state reads, version history, `apply_block`, and `revert_block`.
- Its own query API and query-consistency policy.

The [ERC20](examples.md#erc20) and [Uniswap V3](examples.md#uniswap-v3) examples are complete application mappings. Their source is also available in the repository: [ERC20](https://github.com/nivek3/raven-rs/tree/main/examples/erc20), [Uniswap V3](https://github.com/nivek3/raven-rs/tree/main/examples/uniswap-v3), and [local testkit](https://github.com/nivek3/raven-rs/tree/main/examples/testkit).

Read [Getting started](getting-started.md) for the minimum pipeline, then [Concepts](concepts.md) and [Storage](storage.md) before designing a persistent mapping.
