# RPC block crawler datasource

`RpcBlockCrawler` provides historical/live block acquisition and number/hash
lookups. Sequential windows walk parent hashes from a canonical anchor;
exact-hash reads bind block and log payloads to that hash.

```rust
use raven_evm::{EvmFilter, Filter};
use rpc_block_crawler_datasource::RpcBlockCrawler;

let filter = EvmFilter { blocks: false, logs: Some(Filter::default()) };
let source = RpcBlockCrawler::new(&rpc_url, filter)?;
```

`RpcBlockCrawlerConfig` defaults to `batch_size: 32`, `poll_interval: 1s`.
Use `new_with_config(endpoint, config, filter)` to override them, or
`from_provider(provider, filter)` / `from_provider_with_config(provider, config, filter)`
for an existing Alloy provider. `with_metrics(index)` sets the metric label.

The application configures a filter covering all parsers. The source executes it;
`Pipeline` does not check it. `parser.filter()` and `EvmFilter::merge` are optional.
Sequential and random-access requirements must agree; use a fresh index when
changing selected data. See [filter ownership](../../docs/datasources.md#acquisition-filters-and-parser-matching).
