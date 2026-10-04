# Testing

Raven has unit and integration coverage in the workspace, plus a local testkit that exercises the example mappings through Anvil and PostgreSQL.

## Workspace tests

Run registered test targets from the workspace root:

```sh
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
```

Some PostgreSQL and application-storage tests are ignored because they require a dedicated database configured through `RAVEN_TEST_DATABASE_URL`. Run the three ignored targets explicitly when that database and the offline dependencies are available:

```sh
cargo test -p raven-postgres --test store --locked --offline -- --ignored
cargo test -p raven-postgres --test sql --locked --offline -- --ignored
cargo test -p raven-example-uniswap-v3 --lib --locked --offline -- --ignored
```

The PostgreSQL package README documents the storage cases these tests cover: [verification](https://github.com/nivek3/raven-rs/blob/main/crates/raven-postgres/README.md#verification).

## GitHub Actions

The [Rust workflow](https://github.com/nivek3/raven-rs/blob/main/.github/workflows/rust.yml)
runs on pushes and pull requests to `main`. It checks workspace formatting and
Clippy, runs regular tests and doctests, builds Rust documentation with warnings
denied, and builds this book. A separate job starts a disposable PostgreSQL service
and runs the three ignored database targets listed above.

These CI jobs do not run the full Anvil testkit. Use the local acceptance command
below to verify contract deployment, indexing, restart, reorganization, and clean
replay together.

## Local mapping acceptance

The testkit's full verification command builds and tests the workspace, runs relevant ignored database tests, and exercises ERC20 and Uniswap mappings against its own local Anvil chains:

```sh
cargo run -p raven-testkit --locked --offline -- verify
```

It requires Rust/Cargo, Foundry (`forge`, `cast`, and `anvil`), `psql`, cached Rust dependencies, the Solidity compiler required by the fixtures, and a dedicated `RAVEN_DATABASE_URL`. The command leaves generated schemas in that database and prints a path to a JSON report and process logs. Only a report whose `status` is `passed` confirms the complete workflow.

For narrower workflows, the testkit provides `erc20-rollback` and `uniswap --official-v3-smoke`. Read the [testkit guide](https://github.com/nivek3/raven-rs/blob/main/examples/testkit/README.md) before using either: their database and indexer coverage differs from full verification.

## What to test in an application

An application should test typed entity reads, updates, and deletion; SQL `apply_block` plus `revert_block`, including a restart; historical queries at a known height; and a replacement branch that restores balances, aggregates, membership, and immutable events. The key acceptance property is replay equivalence: after a reorganization and recovery, the projection should match a clean replay of the resulting canonical branch.

## Documentation

This book uses [mdBook](https://rust-lang.github.io/mdBook/). Install the version used by the GitHub workflow, then build or serve from the repository root:

```sh
cargo install mdbook --version '=0.5.4' --locked
mdbook build
mdbook serve --open
```

`mdbook build` writes the rendered book under `book/`. The
[Pages workflow](https://github.com/nivek3/raven-rs/blob/main/.github/workflows/pages.yml)
builds and deploys the book when a `v*` tag is pushed or the workflow is manually
dispatched. Before the first deployment, set **Settings → Pages → Build and
deployment → Source** to **GitHub Actions**. Ensure the `github-pages` environment's
deployment rules allow the intended branch or tag. See GitHub's
[custom Pages workflow guide](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages)
for repository setup. Adding the workflow files alone does not publish the site.
