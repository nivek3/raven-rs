# RPC log crawler datasource

`RpcLogCrawler` uses `eth_getLogs` ranges and emits one batch per block.
Number/hash lookups delegate to `RpcBlockCrawler` with the same filter.

```rust
use raven_evm::{EvmFilter, Filter};
use rpc_log_crawler_datasource::RpcLogCrawler;

let filter = EvmFilter { blocks: false, logs: Some(Filter::default()) };
let source = RpcLogCrawler::new(&rpc_url, filter)?;
```

`RpcLogCrawlerConfig` defaults to `max_block_range: 1000`, `block_concurrency: 10`,
`poll_interval: 1s`. Use `new_with_config(endpoint, config, filter)` to override
them, or `from_provider(provider, filter)` /
`from_provider_with_config(provider, config, filter)` for an existing provider.
`with_metrics(index)` labels both crawlers.

Each request takes one filter object covering the registered parsers.
`parser.filter()` and `EvmFilter::merge` are optional; one broader filter can serve
several parsers. See [filter ownership](../../docs/datasources.md#acquisition-filters-and-parser-matching).

The crawler checks branch continuity and log/block association. When Bloom checks
cannot establish an empty result, it confirms through an exact-hash query.
