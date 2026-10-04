//! Range-based Ethereum log acquisition with hash-bound random access.

mod crawler;
mod error;

pub use alloy_provider::{Provider, ProviderBuilder, RootProvider};
pub use crawler::{RpcLogCrawler, RpcLogCrawlerConfig};
pub use error::RpcLogError;
pub use rpc_block_crawler_datasource::ChainIdentity;
