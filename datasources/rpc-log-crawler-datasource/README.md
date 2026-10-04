# RPC log crawler datasource

`RpcLogCrawler` accelerates log-heavy historical indexing with range-based
`eth_getLogs` requests. It produces one batch per block and delegates reads by
block number or hash to `RpcBlockCrawler`.

Use the default configuration when the RPC provider accepts 1,000-block log ranges:

```rust
use raven_evm::{EvmFilter, Filter};
use rpc_log_crawler_datasource::RpcLogCrawler;

let filter = EvmFilter { blocks: false, logs: Some(Filter::default()) };
let source = RpcLogCrawler::new("https://ethereum-rpc.publicnode.com", filter.clone())?;
```

Tune ranges and block-request concurrency for provider limits:

```rust
use rpc_log_crawler_datasource::{RpcLogCrawler, RpcLogCrawlerConfig};

let source = RpcLogCrawler::new_with_config(
    "https://ethereum-rpc.publicnode.com",
    RpcLogCrawlerConfig {
        max_block_range: 500,
        block_concurrency: 5,
        ..RpcLogCrawlerConfig::default()
    },
    filter,
)?;
```

`from_provider(provider, filter)` and `from_provider_with_config(provider, config, filter)`
accept an existing Alloy provider and explicit acquisition settings.
Pass the acquisition filter when constructing the datasource. With multiple
parsers, use `EvmFilter::merge` to cover their combined demand. The internal
block crawler receives the same filter, so range and hash-addressed reads select
the same data.

The datasource checks branch continuity and associates each log with its block
hash. When a block's Bloom filter cannot establish an empty result, it confirms
the response through an exact-hash query before publishing the batch.
