# Testing

Run commands from the workspace root.

## Workspace tests

```sh
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
```

Database tests require a dedicated `RAVEN_TEST_DATABASE_URL` and run separately:

```sh
cargo test -p raven-postgres --test store --locked --offline -- --ignored
cargo test -p raven-postgres --test sql --locked --offline -- --ignored
cargo test -p raven-example-uniswap-v3 --lib --locked --offline -- --ignored
```

## GitHub Actions

The [Rust workflow](https://github.com/nivek3/raven-rs/blob/main/.github/workflows/rust.yml)
checks formatting, Clippy, tests, doctests, Rust documentation and mdBook on
pushes/PRs to `main`. Its PostgreSQL job runs the ignored tests above.
Full Anvil acceptance is separate.

## Local mapping acceptance

```sh
cargo run -p raven-testkit --locked --offline -- verify
```

This runs workspace/database tests and both mappings through deployment,
indexing, restart, reorg and clean replay. See the
[testkit guide](https://github.com/nivek3/raven-rs/blob/main/examples/testkit/README.md)
for prerequisites, narrower commands and retained-schema queries. Only report
`status: "passed"` confirms completion.

## What to test in an application

Cover entity CRUD, SQL apply/revert after restart, historical queries and a
replacement branch. Recovered balances, relations, aggregates and immutable
events must match a clean replay of that branch.

## Documentation

```sh
cargo install mdbook --version '=0.5.4' --locked
mdbook build
mdbook serve --open
```

Output goes to `book/`. The [Pages workflow](https://github.com/nivek3/raven-rs/blob/main/.github/workflows/pages.yml)
deploys on `v*` tags or manual dispatch. Set **Settings → Pages → Source** to
**GitHub Actions** and allow the intended branch/tag in the `github-pages`
environment. Adding workflows alone does not publish the site.
