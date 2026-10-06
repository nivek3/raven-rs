# Monitoring

Raven uses the `metrics` facade. Install a recorder before indexing; without one,
metrics are not exported and no listener starts.

## Enable Prometheus in the examples

After supplying the example's required configuration, set
`--metrics-listen-addr 127.0.0.1:9464` or
`RAVEN_METRICS_LISTEN_ADDR=127.0.0.1:9464`. The listener is optional; startup failure
exits the example. Each process needs its own address. See
[ERC20 configuration](https://github.com/nivek3/raven-rs/blob/main/examples/erc20/README.md#configuration) or
[Uniswap configuration](https://github.com/nivek3/raven-rs/blob/main/examples/uniswap-v3/README.md#configuration).

```sh
curl http://127.0.0.1:9464/metrics
```

For Prometheus on the same host:

```yaml
scrape_configs:
  - job_name: raven
    scrape_interval: 15s
    static_configs:
      - targets: ["127.0.0.1:9464"]
```

For another host/container, use a reachable listener and target. The HTTP listener
has no authentication; loopback restricts access to the local host.

## Integrate with an application

```toml
raven-metrics = { path = "../raven-rs/crates/raven-metrics", features = ["prometheus"] }
```

Install once per process:

```rust,ignore
raven_metrics::install_prometheus("127.0.0.1:9464".parse()?)?;
```

Give `Pipeline::builder().metrics(index)` and each crawler's `with_metrics(index)`
the same stable name. For direct engine use, call `Engine::with_metrics(index)`.
Clones retain labels; the log crawler labels its exact block crawler too.
Examples use their schema as `index`. Concurrent indexes need distinct names;
the default is `default`. Avoid URLs, hashes, entity IDs and error text in labels.

For another backend, omit the exporter feature, install your own recorder and
call `raven_metrics::describe()`.

## Metric definitions

Counters reset on process restart. Metrics describe observed operations, not
durable accounting: a database commit can finish before its result is observed.

| Metric | Type | Meaning |
| --- | --- | --- |
| `raven_indexed_block_number` | Gauge | Committed height; `-1` before the first commit; decreases on rollback |
| `raven_source_head_block_number` | Gauge | Last successful head observation |
| `raven_source_head_observed_timestamp_seconds` | Gauge | Unix timestamp of that observation |
| `raven_indexing_backlog_blocks` | Gauge | Remaining eligible blocks after start/confirmations |
| `raven_blocks_committed_total` | Counter | Successful commits, including empty blocks and replays |
| `raven_updates_committed_total` | Counter | Updates in committed batches, including replays |
| `raven_blocks_reverted_total` | Counter | Successful rollbacks |
| `raven_reorg_executions_total` | Counter | Resync attempts with at least one successful rollback |
| `raven_reorg_depth_blocks` | Histogram | Successful rollbacks per such attempt, including partial attempts |
| `raven_producer_generations_total` | Counter | Producer generations, including the first |
| `raven_block_processing_duration_seconds` | Histogram | Processing/change preparation, including I/O |
| `raven_block_commit_duration_seconds` | Histogram | Atomic store commit duration |
| `raven_resync_duration_seconds` | Histogram | Preparation, rollback and replay duration |
| `raven_operation_errors_total` | Counter | Failed `process`, `commit`, `resync` calls by `operation` |
| `raven_rpc_requests_total` | Counter | Provider calls completed or cancelled |
| `raven_rpc_request_duration_seconds` | Histogram | Provider call duration |
| `raven_rpc_log_range_splits_total` | Counter | Rejected ranges actually split |

- Duration and RPC request series use `outcome="success"`, `"error"`, or `"cancelled"`. Dropped futures count as cancelled, not operation errors. Nested failures may count at both processing and resync layers.
- Failed commits do not advance committed counters/height. Rollback counts record actual successful transitions; after a position-read failure, the height gauge catches up on the next successful read.
- Backlog runs from the next required block through the eligible head. Start 100, head 120, confirmations 12 and committed 105 gives 3 blocks (106–108); before the first commit it gives 9. No eligible blocks means zero.
- RPC calls carry `crawler="block"` or `"log"` and `method`. A log crawler's exact reads count once under `block`. Configured transport retries remain one provider call. Recovered range errors still count; canonical validation errors do not. Handler RPCs need separate instrumentation.
- Series appear when their paths execute. Head/backlog follow successful observations without extra RPCs. Failed reads retain the last head; use Prometheus `up` to detect an unreachable exporter.

## Useful PromQL

Backlog and head age:

```promql
raven_indexing_backlog_blocks
```

```promql
time() - raven_source_head_observed_timestamp_seconds
```

Commit throughput and RPC error rate:

```promql
sum by (index) (rate(raven_blocks_committed_total[5m]))
```

```promql
sum by (index, crawler, method) (rate(raven_rpc_requests_total{outcome="error"}[5m]))
```

P95 successful processing duration:

```promql
histogram_quantile(0.95,
  sum by (index, le) (
    rate(raven_block_processing_duration_seconds_bucket{outcome="success"}[5m])
  )
)
```

Rollbacks in the last hour: `sum by (index) (increase(raven_blocks_reverted_total[1h]))`.
