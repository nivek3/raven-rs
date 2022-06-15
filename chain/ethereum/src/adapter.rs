use anyhow::Error;
use futures::Future;
use graph::{
    blockchain::{block_types::ChainIdentifier, IngestorError},
    components::ethereum::{EthereumBlock, LightEthereumBlock},
    prelude::{
        async_trait,
        slog::Logger,
        web3::{
            self,
            types::{Block, Log, H256},
        },
    },
};
use std::{marker::Unpin, pin::Pin};

#[async_trait]
pub trait EthereumAdapterTrait: Send + Sync + 'static {
    fn hostname(&self) -> &str;

    /// The `provider` is the URL of the Ethereum node.
    fn provider(&self) -> &str;

    /// Ask the Ethereum node for some identifying information about the Ethereum network it is
    /// connected to.
    async fn net_identifiers(&self) -> Result<ChainIdentifier, Error>;

    /// Get the latest block ptr
    fn latest_block(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = IngestorError> + Send + Unpin>;

    fn latest_block_header(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = web3::types::Block<H256>, Error = IngestorError> + Send>;

    fn load_block(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = Error> + Send>;

    /// Find a block by its hash.
    fn block_by_hash(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send>;

    fn block_by_number(
        &self,
        logger: &Logger,
        block_number: u64,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send>;

    /// Load full information for the specified `block` (in particular, transaction receipts).
    fn load_full_block(
        &self,
        logger: &Logger,
        block: LightEthereumBlock,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<EthereumBlock, IngestorError>> + Send>>;
}
