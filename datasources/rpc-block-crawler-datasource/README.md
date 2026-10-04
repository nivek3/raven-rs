# RPC block crawler datasource

`RpcBlockCrawler` reads EVM block batches through an Alloy provider. It supports
sequential history, live polling and reads by block number or hash.

Use the default configuration for most pipelines:

```rust
use raven_evm::{EvmFilter, Filter};
use rpc_block_crawler_datasource::RpcBlockCrawler;

let filter = EvmFilter { blocks: false, logs: Some(Filter::default()) };
let source = RpcBlockCrawler::new("https://ethereum-rpc.publicnode.com", filter.clone())?;
```

Override bounded acquisition settings only when the RPC provider requires it:

```rust
use rpc_block_crawler_datasource::{RpcBlockCrawler, RpcBlockCrawlerConfig};

let source = RpcBlockCrawler::new_with_config(
    "https://ethereum-rpc.publicnode.com",
    RpcBlockCrawlerConfig {
        batch_size: 64,
        ..RpcBlockCrawlerConfig::default()
    },
    filter,
)?;
```

`from_provider(provider, filter)` and `from_provider_with_config(provider, config, filter)`
accept an existing Alloy provider and explicit acquisition settings.
This is useful for custom transports, middleware and deterministic test providers.

Set the indexing start in the pipeline and pass the acquisition filter when
constructing the datasource. Use `parser.filter()` for one parser or
`EvmFilter::merge` for multiple parsers. Both sequential and random-access sources
must cover the parser demand. Use a fresh index when changing the selected data
so that persisted state covers the same updates throughout its history.

Sequential windows anchor their end block by number and walk parent hashes.
Reads by hash bind both block and log payloads to the requested hash.
