# Datasources

A pipeline needs two related capabilities:

- `Datasource` sequentially emits batches from the next committed height.
- `BlockSource` reads canonical blocks by number and exact blocks by hash for initialization, canonical checks, and reorganization recovery.

`Pipeline` accepts these separately because a custom application may provide them independently. The built-in crawlers implement both, so the same source is normally passed as `.datasource(source.clone())` and `.block_source(source)`.

## RPC block crawler

`RpcBlockCrawler` fetches blocks with payload selected by `EvmFilter`. It supports sequential history, live polling, number lookups, and exact-hash lookups. Use it when handlers need full block payloads or when a mapping makes block-oriented reads.

```rust,ignore
use raven_evm::{EvmFilter, Filter};
use rpc_block_crawler_datasource::RpcBlockCrawler;

let filter = EvmFilter { blocks: true, logs: Some(Filter::default()) };
let source = RpcBlockCrawler::new(&rpc_url, filter)?;
```

`RpcBlockCrawlerConfig` exposes acquisition batch sizing. See the [package README](https://github.com/nivek3/raven-rs/blob/main/datasources/rpc-block-crawler-datasource/README.md) for current defaults and construction variants.

## RPC log crawler

`RpcLogCrawler` uses `eth_getLogs` ranges for log-heavy history, then emits one batch per block. It delegates block-number and exact-hash access to the block crawler. It validates branch continuity and uses hash-bound reads when an empty log result cannot be established from a block's Bloom filter.

```rust,ignore
use raven_evm::{EvmFilter, Filter};
use rpc_log_crawler_datasource::RpcLogCrawler;

let filter = EvmFilter { blocks: false, logs: Some(Filter::default()) };
let source = RpcLogCrawler::new(&rpc_url, filter)?;
```

`RpcLogCrawlerConfig` controls maximum inclusive log range and concurrent block requests. See the [package README](https://github.com/nivek3/raven-rs/blob/main/datasources/rpc-log-crawler-datasource/README.md) for current configuration details.

## Source correctness rules

The selected filter must cover every parser's demand. `LogParser::filter()` provides one parser's demand; merge filters for multiple parsers. Configure the same requirements for sequential and random-access paths, otherwise recovery can replay a different payload from the original stream.

Sources must emit empty blocks where appropriate, but never use empty batches to hide acquisition errors or unavailable data. A requested hash must return that hash if it returns a batch; it must not substitute whatever block is canonical at the same height.

The runner retries source observations classified as `SourceChanged`, `SourceBehind`, or `MissingBlock` by resynchronizing from committed progress. Other source errors terminate the pipeline for the caller to handle.
