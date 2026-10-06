# Raven examples

| Example | Demonstrates |
| --- | --- |
| [ERC20](erc20) | Typed Transfer mapping, balances and history |
| [Uniswap V3](uniswap-v3) | Factory discovery, pricing, positions and aggregates |

Both own their entities, SQL and rollback. `config.rs` defines options;
`events.rs` and `handlers.rs` define mapping logic; `lib.rs` assembles the pipeline;
`main.rs` loads `.env`, monitoring and shutdown handling.

Read the example's configuration or inspect its CLI:

```sh
cargo run -p raven-example-erc20 -- --help
cargo run -p raven-example-uniswap-v3 -- --help
```

## Monitoring

After configuring an example, add `--metrics-listen-addr 127.0.0.1:9464`.
Use different ports for separate processes. Examples use their schema as the
shared engine/crawler `index` label. See [Monitoring](../docs/monitoring.md).

## Local acceptance

Configure a dedicated `RAVEN_DATABASE_URL` in the workspace `.env`, then run:

```sh
cargo run -p raven-testkit --locked --offline -- verify
```

This runs Rust/database tests and both mappings on local Anvil, checking history,
restart, same-height reorg and clean replay. Generated schemas and reports remain
for inspection; owned processes stop afterward. No mainnet fork or RPC is used.

See [testkit](testkit) for prerequisites, reports and narrower commands:
`erc20-rollback` and `official-v3-smoke`. Only report `status: "passed"` confirms
completion. These workflows do not establish production RPC throughput.
