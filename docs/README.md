# Raven

Raven indexes EVM blocks and logs into an application-owned database.

| Raven provides | The application provides |
| --- | --- |
| Canonical processing, reorg recovery and ordered dispatch | Contract selection, start block, parsers and handlers |
| Block-local entity state and atomic state/progress commits | Entity types, SQL tables, history and rollback |
| RPC crawlers, PostgreSQL storage and metrics | Query interface and consistency policy |

Start with [Getting started](getting-started.md), then read
[Concepts](concepts.md) and [Storage](storage.md). The
[ERC20 and Uniswap V3 examples](examples.md) show complete mappings;
[Testing](testing.md) covers local verification.
