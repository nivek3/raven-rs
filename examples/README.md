# Raven examples

Each directory is a standalone workspace package that shows one complete Raven
pipeline. Start with the example closest to the state you want to maintain and adapt
its event definitions and handlers.


| Example                    | Datasource                     | What it demonstrates                                                      |
| -------------------------- | ------------------------------ | ------------------------------------------------------------------------- |
| [erc20](erc20)             | `rpc-log-crawler-datasource`   | Typed ERC20 tables, balances and canonical entity history                 |
| [uniswap-v3](./uniswap-v3) | `rpc-block-crawler-datasource` | Official V3 Factory discovery, entities, pricing and versioned statistics |


The examples use the same source layout:

- `config.rs` defines CLI flags and matching `RAVEN_*` environment variables with Clap.
- `events.rs` defines typed Solidity events.
- `handlers.rs` contains state changes staged in Raven's entity state and committed atomically per block.
- `lib.rs` assembles and runs the pipeline.
- `main.rs` loads `.env`, parses configuration, installs shutdown handling and reports
the final result.

Inspect an example's options from the workspace root:

```sh
cargo run -p raven-example-erc20 -- --help
cargo run -p raven-example-uniswap-v3 -- --help
```

Each example README documents its correctness limits, required infrastructure and
stored entities.

Both examples own their serde entities, native tables, field mappings and
history SQL. The framework commits application state, canonical block status and
indexing progress in one transaction. See the [storage guide](../crates/raven-postgres/README.md)
for application-owned SQL queries and the acceptance workflow below.

## Local acceptance

From the workspace root, explicitly run:

```sh
cargo run -p raven-testkit --locked --offline -- verify
```

Set `RAVEN_DATABASE_URL` in the workspace `.env` to a dedicated test database
before running this command. The command builds and tests the current Rust
workspace, runs PostgreSQL-backed store, application SQL and Uniswap native
mapping tests, and builds local test tokens. It verifies ERC20 history through
the local indexing workflow. It requires Cargo, Rust, Foundry (`forge`, `cast`,
`anvil`), the PostgreSQL `psql` client, locally cached Rust dependencies, and
Solidity 0.8.30. It uses offline builds.

Each run creates its own schemas, such as `raven_<runid>_erc20` and
`raven_<runid>_uniswap_v3`, within that configured database and leaves them in
place for inspection. The JSON report records the actual database and schema
names; use those values in later SQL queries.
The tool starts its own Anvil node, then deploys ERC20 test tokens and the
official Uniswap V3 Factory, position manager and swap router before submitting
transactions to that node's unlocked test accounts. The pinned V3 creation bytecode is checked into
`uniswap-v3/anvil/official-artifacts`; the Factory creates real V3 pools.
It runs both example executables and checks Transfer balances/supply/history,
Swap/Mint decoding, empty blocks, restart, a same-height fork, rollback and equality
with fresh canonical replay. It also compares every native ERC20 field with the
handler entities, verifies historical snapshots, and compares all relational
versions after reorg with clean replay. V3 liquidity and swaps execute through
the official contracts.
Both examples are checked for lowercase hexadecimal text. All Uniswap native
entity versions are also compared with clean canonical replay.

Afterward, the owned processes stop while the run's dynamic schemas remain in
the configured database. Logs and a JSON report remain in the temporary directory
printed by the tool. Only `status: "passed"` confirms that the checks completed
successfully.

For a quick contract-only check that skips workspace tests and PostgreSQL:

```sh
cargo run -p raven-testkit --locked --offline -- official-v3-smoke
```

Both workflows use a local Anvil chain without a mainnet fork or a mainnet RPC.
They verify local contract/indexer behavior, not production RPC performance. See
the [Uniswap verification guide](uniswap-v3/README.md#verification) for coverage.

The Rust tool lives in [testkit](testkit) and
provides three subcommands, including the
[ERC20 rollback demonstration](erc20/README.md#run-the-rollback-example).
