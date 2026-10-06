# Pipeline configuration

Applications map their CLI/environment settings to the pipeline, sources,
storage and handlers.

## Pipeline settings

| Builder method | Meaning |
| --- | --- |
| `from_block` | Start of a fresh index; default `0` |
| `finality_policy` | `Head` by default; examples use `Confirmations(12)` |
| `run_options` | Start block, channel capacity and engine polling interval |
| `datasource`, `block_source` | Sequential and random-access acquisition |
| `store` | Persistent `ChainStore` |
| `cancellation_token` | Cooperative shutdown signal |
| `metrics` | Stable index label; default `default` |

`RunOptions` defaults to `start_block: 0`, `channel_capacity: 64` and
`poll_interval: 1s`. Capacity and interval must be positive. `from_block` changes
only the start; `run_options` replaces all three fields. The last call determines
the configured start for a fresh index.

Confirmations delay processing without replacing canonical checks or rollback.
Increasing them retains already committed blocks on the same canonical branch.

## Example CLI conventions

CLI flags override matching `RAVEN_*` variables. Both examples load `.env` from
the working directory; do not execute it with `source`. They require RPC URL,
database URL, schema, network name and start block, plus a token or Factory address.
See [example configuration](examples.md) or run the executable with `--help`.

Prometheus is disabled unless `--metrics-listen-addr` or
`RAVEN_METRICS_LISTEN_ADDR` is set. See [Monitoring](monitoring.md).

## Startup and shutdown semantics

Raven records chain identity and start metadata after verifying the starting
branch, and rejects a different identity on restart. Persisted start/progress
win over new configuration, even after rollback clears the pointer. A different
start requires a fresh index or an application-managed migration.

Divergent history triggers rollback/replay. Applications handle SIGINT/SIGTERM
and pass a cancellation token; the examples do this automatically.
