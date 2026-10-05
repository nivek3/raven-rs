# Monitoring

Raven records canonical indexing and RPC metrics through the `metrics` facade.
Applications install a recorder before starting their pipelines. Without a
recorder, no metrics are exported and no HTTP listener is started.

## Enable Prometheus in the examples

Both indexing examples accept `--metrics-listen-addr` or
`RAVEN_METRICS_LISTEN_ADDR`. The option has no default and is added to an otherwise
configured indexer. Supply the required settings through CLI flags, environment
variables, or `.env` before starting it:

| Required setting | CLI flag | Environment variable |
| --- | --- | --- |
| RPC endpoint, both examples | `--rpc-url` | `RAVEN_RPC_URL` |
| PostgreSQL connection, both examples | `--database-url` | `RAVEN_DATABASE_URL` |
| Index schema, both examples | `--schema` | `RAVEN_SCHEMA` |
| Network name, both examples | `--network-name` | `RAVEN_NETWORK_NAME` |
| First indexed block, both examples | `--start-block` | `RAVEN_START_BLOCK` |
| Token contract, ERC20 | `--token` | `RAVEN_TOKEN` |
| Factory contract, Uniswap V3 | `--factory` | `RAVEN_FACTORY` |

See the [ERC20 configuration](https://github.com/nivek3/raven-rs/blob/main/examples/erc20/README.md#configuration) and
[Uniswap V3 configuration](https://github.com/nivek3/raven-rs/blob/main/examples/uniswap-v3/README.md#configuration) for
complete startup instructions and optional settings. An ERC20 startup error
listing `--network-name` and `--token` means those required values are missing;
provide them through the flags or their environment variables.

Once configured, add this flag to the example's startup command:

```sh
--metrics-listen-addr 127.0.0.1:9464
```

The equivalent environment setting is:

```dotenv
RAVEN_METRICS_LISTEN_ADDR=127.0.0.1:9464
```

The examples install the recorder before starting the indexer and use their
PostgreSQL schema name as the `index` label. Each process needs its own listen
address. A configured exporter that cannot start causes the example to exit with
failure.

Inspect the scrape output:

```sh
curl http://127.0.0.1:9464/metrics
```

For a Prometheus process running on the same host:

```yaml
scrape_configs:
  - job_name: raven
    scrape_interval: 15s
    metrics_path: /metrics
    static_configs:
      - targets: ["127.0.0.1:9464"]
```

If Prometheus runs on another host or in a container, configure a reachable
listener address and target. The listener has no authentication; loopback limits
access to the local host. The exporter responds to GET requests, including
`/metrics`.

## Integrate with an application

Add a local dependency with the exporter feature:

```toml
raven-metrics = { path = "../raven-rs/crates/raven-metrics", features = ["prometheus"] }
```

Before creating the pipeline, install the recorder once and assign a stable name
to the pipeline and its sources:

```rust
raven_metrics::install_prometheus("127.0.0.1:9464".parse()?)?;

let source = RpcLogCrawler::new(&rpc_url, parser.filter())?
    .with_metrics("token-balances");

let mut pipeline = Pipeline::builder()
    .metrics("token-balances")
    .datasource(source.clone())
    .block_source(source)
    .store(store)
    .from_block(start_block)
    .finality_policy(FinalityPolicy::Confirmations(12))
    .parser(parser, (handler,))
    .build()?;

pipeline.run().await?;
```

The fragment assumes the imports and application values from
[Getting started](getting-started.md). When using `Engine` directly, configure
`Engine::with_metrics(index)` instead. Both RPC crawlers provide
`with_metrics(index)`. A log crawler also passes its name to its exact block
crawler, and clones retain that name.

Every index has an `index` label. The default is `default`; concurrent engines
in one process must use different names to keep their gauges distinct. Keep names
stable across restarts. RPC metrics also have `crawler="block"` or `"log"` and a
fixed JSON-RPC `method`. Do not use RPC URLs, hashes, entity IDs or error text as
labels.

For another backend, omit the `prometheus` feature and install your own `metrics`
recorder, then call `raven_metrics::describe()`. The engine and datasource crates
do not start exporters themselves.

## Metric definitions

All counters cover observations made by the current process and reset on process
restart. The engine restores the indexed height from persisted progress. Metrics
are not a durable accounting ledger: a database commit may complete even if the
process dies before observing its result.

| Metric | Type | Meaning |
| --- | --- | --- |
| `raven_indexed_block_number` | Gauge | Current committed height; `-1` when no block is committed. Decreases during rollback. |
| `raven_source_head_block_number` | Gauge | Most recent successful head observation, which may also decrease. |
| `raven_source_head_observed_timestamp_seconds` | Gauge | Unix timestamp of that observation. |
| `raven_indexing_backlog_blocks` | Gauge | Eligible blocks remaining after accounting for the configured start and confirmations. |
| `raven_blocks_committed_total` | Counter | Successful atomic block commits, including empty blocks and replayed blocks. |
| `raven_updates_committed_total` | Counter | Updates in successfully committed batches, including replays. |
| `raven_blocks_reverted_total` | Counter | Successful block rollbacks. |
| `raven_reorg_executions_total` | Counter | Resync attempts with at least one successful rollback. |
| `raven_reorg_depth_blocks` | Histogram | Number of successful rollbacks in each such attempt, including partial attempts. |
| `raven_producer_generations_total` | Counter | Datasource generations started, including the first. |
| `raven_block_processing_duration_seconds` | Histogram | Parser/handler execution and entity change preparation, including their I/O. |
| `raven_block_commit_duration_seconds` | Histogram | Atomic store commit duration. |
| `raven_resync_duration_seconds` | Histogram | Branch preparation, rollback and replay duration. |
| `raven_operation_errors_total` | Counter | Failed `process`, `commit` or `resync` calls, selected by `operation`. |
| `raven_rpc_requests_total` | Counter | Provider calls that completed or were cancelled. |
| `raven_rpc_request_duration_seconds` | Histogram | Duration of each such provider call. |
| `raven_rpc_log_range_splits_total` | Counter | Rejected log ranges actually split into smaller ranges. |

Duration histograms and RPC counters carry
`outcome="success"`, `"error"` or `"cancelled"`. An unfinished operation whose
future is dropped is cancelled, and does not increment the operation error
counter. A failed commit does not increment committed counters or advance the
indexed height. A processing error can also fail its enclosing resync; the
operation error counter describes calls at each named layer.

Backlog counts from the next required height through the processable head,
inclusive. If the index starts at block 100, confirmations are 12, the observed
head is 120 and the committed height is 105, backlog is 3: blocks 106–108. Before
any commit it is 9: blocks 100–108. It is zero when no block is eligible. The
head and backlog series appear after successful head observation; monitoring
does not issue additional RPC requests.

Resync can fill a gap or verify a finite stream without a reorg. Only successful
rollbacks count as a reorg execution. A failed or cancelled attempt records the
depth it actually rolled back; a later retry is a separate attempt. A successful
rollback followed by a failed store position read increments the rollback count,
but the height gauge catches up on the next successful position read.

RPC metrics cover calls made by the framework crawlers. A log crawler's exact
reads are labelled `crawler="block"` and are counted once. A provider call
includes any transport retries configured by the application; it is not a count
of individual HTTP attempts. Provider errors count even when a rejected log
range is recovered by splitting it. Canonical validation errors and a closed
batch channel are not provider errors. RPC calls made directly inside application
handlers need their own instrumentation.

Metrics are created as their paths execute. A missing counter before the first
event does not imply a failure. Use Prometheus's `up` metric to detect a stopped
or unreachable exporter. Source head values retain the last successful
observation when later reads fail.

## Useful PromQL

Eligible indexing backlog:

```promql
raven_indexing_backlog_blocks
```

Seconds since the latest successful head observation:

```promql
time() - raven_source_head_observed_timestamp_seconds
```

Block commit throughput per index:

```promql
sum by (index) (rate(raven_blocks_committed_total[5m]))
```

RPC errors per second:

```promql
sum by (index, crawler, method) (
  rate(raven_rpc_requests_total{outcome="error"}[5m])
)
```

P95 successful block processing duration:

```promql
histogram_quantile(0.95,
  sum by (index, le) (
    rate(raven_block_processing_duration_seconds_bucket{outcome="success"}[5m])
  )
)
```

Successful rollbacks in the last hour:

```promql
sum by (index) (increase(raven_blocks_reverted_total[1h]))
```
