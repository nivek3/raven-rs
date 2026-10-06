# Raven

[![Rust CI](https://github.com/nivek3/raven-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/nivek3/raven-rs/actions/workflows/rust.yml)

A lightweight Rust framework for EVM indexing. Raven processes blocks and logs in
canonical order and commits application state with indexing progress. Applications
own their handlers, SQL tables, history, queries and rollback logic.

## Usage

The workspace crates use local path dependencies. This fragment assumes your
configuration, handler and application-backed store:

```rust
use raven_engine::FinalityPolicy;
use raven_evm::{sol, LogParser, Parser, Pipeline};
use rpc_log_crawler_datasource::RpcLogCrawler;

sol! { event Transfer(address indexed from, address indexed to, uint256 value); }

let parser = LogParser::<Transfer>::new(token_address);
let source = RpcLogCrawler::new(&rpc_url, parser.filter())?;
let mut pipeline = Pipeline::builder()
    .datasource(source.clone())
    .block_source(source)
    .store(store)
    .from_block(start_block)
    .finality_policy(FinalityPolicy::Confirmations(12))
    .parser(parser, (handler,))
    .build()?;
pipeline.run().await?;
```

Start with [Getting started](docs/getting-started.md) or the complete
[ERC20 example](examples/erc20). Source filters must cover every parser's needs;
filter merging is optional.

## Packages

| Package | Purpose |
| --- | --- |
| [raven-engine](crates/raven-engine) | Canonical processing, recovery and store contracts |
| [raven-evm](crates/raven-evm) | EVM updates, typed parsers and pipeline assembly |
| [raven-postgres](crates/raven-postgres) | Atomic application writes and progress |
| [raven-metrics](crates/raven-metrics) | Framework metrics and optional Prometheus export |
| [rpc-block-crawler-datasource](datasources/rpc-block-crawler-datasource) | Block-oriented RPC acquisition |
| [rpc-log-crawler-datasource](datasources/rpc-log-crawler-datasource) | Range-based log acquisition |
| [raven-testkit](examples/testkit) | Local Anvil/PostgreSQL verification |

## Documentation

Read the [guide](docs/README.md) for [processing and reorgs](docs/concepts.md),
[entities](docs/entities.md), [storage](docs/storage.md), [datasources](docs/datasources.md),
[configuration](docs/configuration.md) and [monitoring](docs/monitoring.md).
See [Testing](docs/testing.md) for test commands and mdBook setup.

## Examples

- [ERC20](examples/erc20): metadata, balances, supply and Transfer history.
- [Uniswap V3](examples/uniswap-v3): pool discovery, pricing, NFT positions and aggregates.

Run [local acceptance](examples/README.md#local-acceptance) with a dedicated test
database. It uses local Anvil without a mainnet fork.
