use crate::{
    adapter::EthereumAdapterTrait, block::BlockFinality, ethereum_adapter::EthereumAdapter,
    network::EthereumNetworkAdapters,
};
use graph::{
    blockchain::{
        BlockHash, BlockPtr, Blockchain, IngestorAdapter as IngestorAdapterTrait, IngestorError,
    },
    log::factory::LoggerFactory,
    prelude::{
        async_trait,
        slog::{o, Logger},
        BlockNumber, ChainStore, Future01CompatExt,
    },
};
use std::sync::Arc;

pub struct Chain {
    logger_factory: LoggerFactory,
    pub name: String,
    eth_adapters: Arc<EthereumNetworkAdapters>,
    chain_store: Arc<dyn ChainStore>,
    ancestor_count: BlockNumber,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chain: ethereum")
    }
}

impl Chain {
    pub fn new(
        logger_factory: LoggerFactory,
        name: String,
        ancestor_count: BlockNumber,
        eth_adapters: EthereumNetworkAdapters,
        chain_store: Arc<dyn ChainStore>,
    ) -> Self {
        Chain {
            logger_factory,
            name,
            eth_adapters: Arc::new(eth_adapters),
            ancestor_count,
            chain_store,
        }
    }
}

#[async_trait]
impl Blockchain for Chain {
    type IngestorAdapter = IngestorAdapter;
    type Block = BlockFinality;

    // type TriggerFilter = crate::adapter::TriggerFilter;

    fn ingestor_adapter(&self) -> Arc<Self::IngestorAdapter> {
        let eth_adapter = self.eth_adapters.cheapest().unwrap().clone();
        let logger = self
            .logger_factory
            .component_logger("BlockIngestor")
            .new(o!("provider" => eth_adapter.provider().to_string()));
        let adapter = IngestorAdapter {
            logger,
            eth_adapter,
            ancestor_count: self.ancestor_count,
            chain_store: self.chain_store.clone(),
        };
        Arc::new(adapter)
    }
}

pub struct IngestorAdapter {
    logger: Logger,
    ancestor_count: i32,
    eth_adapter: Arc<EthereumAdapter>,
    chain_store: Arc<dyn ChainStore>,
}

#[async_trait]
impl IngestorAdapterTrait<Chain> for IngestorAdapter {
    fn logger(&self) -> &Logger {
        &self.logger
    }

    fn ancestor_count(&self) -> BlockNumber {
        self.ancestor_count
    }

    async fn latest_block(&self) -> Result<BlockPtr, IngestorError> {
        self.eth_adapter
            .latest_block_header(&self.logger)
            .compat()
            .await
            .map(|block| block.into())
    }

    async fn ingest_block(
        &self,
        _block_hash: &BlockHash,
    ) -> Result<Option<BlockHash>, IngestorError> {
        unimplemented!()
    }

    fn chain_head_ptr(&self) -> Result<Option<BlockPtr>, anyhow::Error> {
        self.chain_store.chain_head_ptr()
    }
}
