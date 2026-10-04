//! Hash-bound Ethereum RPC acquisition for sequential and canonical-engine paths.

mod crawler;
mod error;

pub use alloy_provider::{Provider, ProviderBuilder, RootProvider};
pub use crawler::{ChainIdentity, RpcBlockCrawler, RpcBlockCrawlerConfig};
pub use error::RpcError;
