# Getting started

Use the workspace crates as local path dependencies. The
[ERC20 example](https://github.com/nivek3/raven-rs/tree/main/examples/erc20)
is the smallest complete mapping.

## Run the ERC20 example from this workspace

Replace the placeholders below. Use a dedicated schema and a start block that
includes the token's complete Transfer history:

```sh
export RAVEN_RPC_URL="<rpc-url>"
export RAVEN_DATABASE_URL="<database-url>"
export RAVEN_SCHEMA="<dedicated-schema>"
export RAVEN_NETWORK_NAME="<network-name>"
export RAVEN_TOKEN="<erc20-address>"
export RAVEN_START_BLOCK="<first-required-block>"
cargo run -p raven-example-erc20 --locked
```

For all options, run `cargo run -p raven-example-erc20 -- --help`.

## Build a pipeline

Define an Alloy event, configure a source, implement a handler and
[application storage](storage.md), then register them. This fragment assumes your
configuration, `store` and `transfer_handler`:

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

The source must cover every parser's needs. `parser.filter()` and filter merging
are optional configuration helpers; see [filter ownership](datasources.md#acquisition-filters-and-parser-matching).
Starting late requires bootstrapped state or produces a partial projection.
`Confirmations(12)` permits block 88 at head 100; it delays processing without
replacing canonical checks or rollback.

Next: [Entities](entities.md), [Concepts](concepts.md), and [Configuration](configuration.md).
