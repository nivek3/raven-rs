//! Command-line and environment configuration for the ERC-20 example.

use std::num::{NonZeroU64, NonZeroUsize};

use alloy_primitives::Address;
use clap::Parser;

/// Index one ERC-20 token into OpenZeppelin-compatible entities in PostgreSQL.
///
/// Every option can be supplied as a CLI flag or through its `RAVEN_*`
/// environment variable. CLI flags take precedence when both are present.
#[derive(Parser)]
#[command(name = "raven-example-erc20", version, about)]
pub struct Config {
    /// HTTP(S) Ethereum JSON-RPC endpoint.
    #[arg(
        long,
        env = "RAVEN_RPC_URL",
        value_name = "URL",
        hide_env_values = true
    )]
    pub rpc_url: String,

    /// PostgreSQL connection URL.
    #[arg(
        long,
        env = "RAVEN_DATABASE_URL",
        value_name = "URL",
        hide_env_values = true
    )]
    pub database_url: String,

    /// Dedicated PostgreSQL schema owned by this index.
    #[arg(long, env = "RAVEN_SCHEMA")]
    pub schema: String,

    /// Configured network name, for example ethereum-mainnet or anvil.
    #[arg(long, env = "RAVEN_NETWORK_NAME")]
    pub network_name: String,

    /// ERC-20 contract address.
    #[arg(long, env = "RAVEN_TOKEN")]
    pub token: Address,

    /// First block to index.
    #[arg(long, env = "RAVEN_START_BLOCK")]
    pub start_block: u64,

    /// Number of head blocks to leave unprocessed.
    #[arg(long, env = "RAVEN_CONFIRMATIONS", default_value_t = 12)]
    pub confirmations: u64,

    /// Maximum inclusive block range for one `eth_getLogs` request.
    #[arg(long, env = "RAVEN_MAX_BLOCK_RANGE", default_value = "1000")]
    pub max_block_range: NonZeroU64,

    /// Maximum number of concurrent block requests.
    #[arg(long, env = "RAVEN_BLOCK_CONCURRENCY", default_value = "10")]
    pub block_concurrency: NonZeroUsize,
}
