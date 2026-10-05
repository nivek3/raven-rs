//! Command-line and environment configuration for the Uniswap V3 example.

use std::net::SocketAddr;

use alloy_primitives::Address;
use clap::Parser;

/// Index a Uniswap V3 Factory and its pools into PostgreSQL.
///
/// Every option can be supplied as a CLI flag or through its `RAVEN_*`
/// environment variable. CLI flags take precedence when both are present.
#[derive(Parser)]
#[command(name = "raven-example-uniswap-v3", version, about)]
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

    /// Uniswap V3 factory contract address.
    #[arg(long, env = "RAVEN_FACTORY")]
    pub factory: Address,

    /// NFT position manager indexed by this application.
    #[arg(
        long,
        env = "RAVEN_POSITION_MANAGER",
        default_value = "0xc36442b4a4522e871399cd717abdd847ab11fe88"
    )]
    pub position_manager: Address,

    /// Application pricing policy; defaults to the Ethereum configuration.
    #[arg(long, env = "RAVEN_CHAIN_CONFIG")]
    pub chain_config: Option<std::path::PathBuf>,

    /// First block to index.
    #[arg(long, env = "RAVEN_START_BLOCK")]
    pub start_block: u64,

    /// Number of head blocks to leave unprocessed.
    #[arg(long, env = "RAVEN_CONFIRMATIONS", default_value_t = 12)]
    pub confirmations: u64,

    /// Optional address for the Prometheus metrics listener.
    #[arg(long, env = "RAVEN_METRICS_LISTEN_ADDR")]
    pub metrics_listen_addr: Option<SocketAddr>,
}
