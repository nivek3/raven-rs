# Examples

Each example owns its entities, SQL, history, storage adapter and configuration.

## ERC20

[ERC20](https://github.com/nivek3/raven-rs/tree/main/examples/erc20) is the smallest
complete mapping: typed Transfer logs, metadata, balances, supply and versioned
PostgreSQL history through `RpcLogCrawler`.

## Uniswap V3

[Uniswap V3](https://github.com/nivek3/raven-rs/tree/main/examples/uniswap-v3)
demonstrates custom acquisition, pool discovery, pricing, NFT positions and
aggregates. Its README records mapping-specific assumptions and limitations.

## Local testkit

The [testkit](https://github.com/nivek3/raven-rs/blob/main/examples/testkit/README.md)
checks both mappings on local Anvil with a dedicated PostgreSQL database. It
retains generated schemas and reports, and checks restart, reorg and clean replay.
These checks do not establish production RPC throughput.
