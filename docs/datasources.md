# Datasources

A pipeline uses `Datasource` for sequential batches and `BlockSource` for
canonical headers and exact-hash recovery. Built-in crawlers implement both:
pass the same source to `.datasource(source.clone()).block_source(source)`.

## RPC block crawler

`RpcBlockCrawler` acquires blocks with selected full-block/log payloads. It
supports historical catch-up, live polling and exact-hash reads. See its
[configuration](https://github.com/nivek3/raven-rs/blob/main/datasources/rpc-block-crawler-datasource/README.md).

## RPC log crawler

`RpcLogCrawler` uses `eth_getLogs` ranges for log-heavy history and emits one batch
per block. Random access delegates to the block crawler. Branch checks and
exact-hash queries confirm empty results when Bloom checks cannot. See its
[configuration](https://github.com/nivek3/raven-rs/blob/main/datasources/rpc-log-crawler-datasource/README.md).

## Acquisition filters and parser matching

**The source executes the acquisition filter; the application ensures it covers
every parser's needs.** `Pipeline` does not derive or check filters.
`Parser::filter()` and `EvmFilter::merge()` are optional helpers. One broader
filter can serve several parsers, such as Transfer and Approval from one contract:

```rust,ignore
let filter = EvmFilter {
    blocks: false,
    logs: Some(Filter::new().address(token_address)),
};
let source = RpcLogCrawler::new(&rpc_url, filter)?;
```

Each `eth_getLogs` request takes one filter object, which can contain several
addresses and candidate topics. Parser count does not determine request count.
`blocks` selects full-block updates; `logs: None` selects no logs and
`Some(Filter::default())` selects all logs. Headers are always retained.

## Source correctness rules

Sources must return complete, unique, ordered updates tied to the exact batch
header, including empty blocks. Sequential and random-access paths must use
equivalent requirements. Built-in crawlers fix those requirements for the run.

Exact-hash reads must not substitute the current block at that height.
Unavailable data and acquisition failures must not become empty batches.
The engine checks chain headers; it trusts source payload completeness and order.
See [processing order and recovery](concepts.md#data-completeness-and-ordering).

The runner retries `SourceChanged`, `SourceBehind` and `MissingBlock` observations
from committed progress. Other errors return to the application.
