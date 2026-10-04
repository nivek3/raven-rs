# Raven

[![Rust CI](https://github.com/nivek3/raven-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/nivek3/raven-rs/actions/workflows/rust.yml)

A lightweight Rust framework for EVM indexing. Raven acquires blocks and logs,
decodes typed events with Alloy, and processes updates in canonical block order.
It coordinates indexing progress with application writes and rollback on chain reorgs.

Applications define their Rust handlers, entity tables, version history and SQL
queries. Raven provides the pipeline and storage contracts that connect them.

## Documentation

The [Raven guide](docs/README.md) covers [getting started](docs/getting-started.md),
[block processing and reorgs](docs/concepts.md), [typed entities](docs/entities.md),
and [application storage](docs/storage.md). Follow the
[testing guide](docs/testing.md) for PostgreSQL tests and local testkit verification.

The guide is built with mdBook. To preview it locally from the workspace root:

```sh
cargo install mdbook --version '=0.5.4' --locked
mdbook serve --open
```

`mdbook build` writes the static site to `book/`. The GitHub Actions workflows
check the book and prepare GitHub Pages deployment. See the
[documentation workflow](docs/testing.md#documentation) for repository setup.

## Components

- **Pipeline** connects datasources, parsers, handlers and a store. Each update
  is dispatched to parsers and handlers in registration order.
- **Datasources** use Alloy providers for historical catch-up, live polling and
  exact-hash reads during gap recovery and reorgs.
- **Parsers and handlers** decode logs or consume full blocks, then stage entity
  changes. Later handlers in the same block can read earlier changes.
- **Engine and storage** resynchronize the canonical chain and coordinate atomic
  application writes and rollback with progress. PostgreSQL storage is included.

## Usage

Define an event with Alloy and configure a datasource from its parser:

```rust
use raven_engine::FinalityPolicy;
use raven_evm::{sol, LogParser, Parser, Pipeline};
use rpc_log_crawler_datasource::RpcLogCrawler;

sol! {
    event Transfer(address indexed from, address indexed to, uint256 value);
}

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

This fragment assumes application configuration, an event handler and a store
using your [`PostgresStorage`](crates/raven-postgres/README.md#application-storage)
implementation. See the [ERC20 example](examples/erc20) for a complete pipeline.

Log constraints use Alloy's `Filter`. Configure each datasource with the logs and
block payload its parsers require; block headers are retained even when no updates
match.

## Packages

| Package | Purpose |
| --- | --- |
| [`raven-engine`](crates/raven-engine) | Canonical resynchronization, block ingestion, handlers and store contracts |
| [`raven-evm`](crates/raven-evm) | EVM updates, typed parsers and pipeline assembly |
| [`raven-postgres`](crates/raven-postgres) | PostgreSQL progress and atomic application storage operations |
| [`rpc-block-crawler-datasource`](datasources/rpc-block-crawler-datasource) | Block-oriented RPC acquisition |
| [`rpc-log-crawler-datasource`](datasources/rpc-log-crawler-datasource) | Range-based log acquisition with exact-hash recovery |
| [`raven-testkit`](examples/testkit) | Local Anvil/PostgreSQL verification and rollback demonstrations |

Use the workspace packages through local path dependencies.

## Examples

| Example | What it demonstrates |
| --- | --- |
| [ERC20](examples/erc20) | Token metadata, balances, supply, Transfer history and historical SQL |
| [Uniswap V3](examples/uniswap-v3) | Factory discovery, pool pricing, NFT positions and day/hour aggregates |

The example READMEs describe configuration and indexing start requirements.
From the workspace root, inspect the CLI options with:

```sh
cargo run -p raven-example-erc20 -- --help
cargo run -p raven-example-uniswap-v3 -- --help
```

See the [examples guide](examples/README.md) for local Anvil deployment,
PostgreSQL verification and rollback/replay checks. Local acceptance uses
`RAVEN_DATABASE_URL` from `.env` and runs without a mainnet fork or mainnet RPC.

## Storage

`raven-postgres` stores canonical blocks and network progress. Applications own
their entity relations and implement reads, writes and rollback on Raven's SQL
connection. Business state and progress commit in the same transaction.

Handlers stage changes in memory for one block. Applications query their native
tables with SQLx or a SQL client, including historical versions. See the
[storage guide](crates/raven-postgres/README.md) for the storage contract and query
examples. Use a fresh schema when changing the indexed contracts or mapping rules.

## Entity access

Applications implement `Entity` for their serializable Rust entity types. The
stable type name identifies the application's storage mapping; `id()` identifies
one instance. Import `EntityStoreExt` to use typed access in handlers:

```rust
use raven_engine::{Entity, EntityStore, EntityStoreExt, RavenResult};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Balance {
    id: String,
    amount: String,
}

impl Entity for Balance {
    const ENTITY_NAME: &'static str = "Balance";

    fn id(&self) -> &str {
        &self.id
    }
}

async fn update(entities: &mut dyn EntityStore, id: &str) -> RavenResult<()> {
    let mut balance = entities.load::<Balance>(id).await?
        .unwrap_or_else(|| Balance { id: id.to_owned(), amount: "0".to_owned() });
    balance.amount = "100".to_owned();
    entities.save(&balance).await?;
    Ok(())
}
```

`load` returns `None` for a missing entity and an error for invalid serialized
values or mismatched IDs. `save` stages a complete replacement; `remove::<Balance>`
stages deletion. All three use the same block-local state as raw `get/put/delete`.
`EntityValue` is the JSON representation used by the raw store interface and
storage adapters. Applications still own SQL mappings, history and rollback.

## Testing

Run the registered workspace tests:

```sh
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
```

Database tests require a dedicated `RAVEN_TEST_DATABASE_URL` and are run explicitly.
See [PostgreSQL verification](crates/raven-postgres/README.md#verification) and the
[local acceptance workflow](examples/README.md#local-acceptance).
