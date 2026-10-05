# Pipeline configuration

Raven's framework configuration is assembled through `Pipeline::builder()`. An application normally builds its own CLI and environment layer, then maps those values to the builder, datasource, storage, and handlers.

## Pipeline settings

- `from_block`: the initial height recorded with network metadata. Raven resumes from committed progress on later runs.
- `finality_policy`: `Head` or `Confirmations(n)`. A confirmation count delays processing; it does not replace canonical verification or rollback.
- `datasource` and `block_source`: sequential and random-access chain access.
- `store`: the `ChainStore` holding network metadata and progress.
- `cancellation_token`: shared graceful-stop signal.

The engine's `RunOptions` has `start_block`, `channel_capacity`, and `poll_interval` fields. It defaults to start block `0`, channel capacity `64`, and a one-second chain polling interval. A custom run option must use a positive channel capacity and polling interval.

## Example CLI conventions

The examples use Clap. Every option can be supplied as a flag or matching `RAVEN_*` environment variable, with a flag taking precedence. Both require an HTTP(S) RPC URL, PostgreSQL URL, dedicated schema, network name, and start block.

The ERC20 example also requires a token address and accepts confirmations, maximum log range, and block-request concurrency. The Uniswap V3 example requires a factory address and accepts a position manager and chain-policy JSON. Use the executable's built-in help for the current option set:

```sh
cargo run -p raven-example-erc20 -- --help
cargo run -p raven-example-uniswap-v3 -- --help
```

The examples load `.env` from the working directory. Treat it as configuration data, not a shell script; do not run `source .env`. Keep database credentials out of documentation and version control.

## Startup and shutdown semantics

On a new index, Raven records immutable chain identity and start metadata only after it verifies the selected start branch. A later source with a different identity fails rather than attaching existing tables to a different chain.

On restart, Raven retains recorded network metadata and derives the next height from committed progress. If current source history diverges, it rolls back and replays. SIGINT/SIGTERM handling is an application concern; the examples pass a shutdown token into the pipeline, which cancels the active producer and lets it exit cleanly.
