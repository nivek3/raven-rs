# Getting started

Raven is currently a workspace of local Rust crates. Add the crates as path dependencies, or start by copying the structure of the [ERC20 example](https://github.com/nivek3/raven-rs/tree/main/examples/erc20).

## Run the ERC20 example from this workspace

From the workspace root, inspect the complete option set first:

```sh
cargo run -p raven-example-erc20 -- --help
```

Provide your own RPC and database configuration through the environment. Set
`TOKEN_ADDRESS` and `START_BLOCK` to the ERC20 contract and a start height that
includes all history required by the projection. Choose a dedicated, unused
PostgreSQL schema for this index and a network name appropriate to the source.

```sh
export RAVEN_RPC_URL="<your-rpc-url>"
export RAVEN_DATABASE_URL="<your-database-url>"
export TOKEN_ADDRESS="<erc20-address>"
export START_BLOCK="<first-required-block>"

RAVEN_TOKEN="$TOKEN_ADDRESS" \
RAVEN_START_BLOCK="$START_BLOCK" \
RAVEN_SCHEMA="<dedicated-schema>" \
RAVEN_NETWORK_NAME="<network-name>" \
  cargo run -p raven-example-erc20 --locked
```

The sample pipeline below is a framework fragment. `store`,
`transfer_handler`, `token_address`, `rpc_url`, and `start_block` stand for
application configuration and components; it is not a complete executable by
itself.

The smallest useful index has five pieces:

1. An Alloy event definition and a `LogParser`.
2. A datasource configured to acquire the data the parser needs.
3. A handler that changes application entities.
4. A `ChainStore`, commonly `PostgresChainStore` with an application `PostgresStorage` implementation.
5. A `Pipeline` that connects them and selects a start and finality policy.

```rust,ignore
use raven_engine::{CancellationToken, FinalityPolicy};
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
    .cancellation_token(CancellationToken::new())
    .parser(parser, (transfer_handler,))
    .build()?;
pipeline.run().await?;
```

This example uses `parser.filter()` to configure the source. Applications can also configure the source directly, such as selecting every log from one contract for several event parsers. The application must ensure the source covers all registered parsers' needs; `Pipeline` does not configure or check acquisition filters. See [Acquisition filters and parser matching](datasources.md#acquisition-filters-and-parser-matching) for the responsibility split and optional filter merging.

Choose `start_block` at or before the first event needed to derive correct state. Starting after contract discovery or creation events gives a partial projection unless the application explicitly imports the missing state.

`FinalityPolicy::Confirmations(12)` means a head at height 100 permits block 88. It is an ingestion delay, not a replacement for canonical checks and rollback.

The next steps are [Entities](entities.md), [Storage](storage.md), and [Datasources](datasources.md).