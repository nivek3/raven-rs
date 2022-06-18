use crate::{
    adapter::EthereumAdapterTrait, ethereum_adapter::EthereumAdapter,
    network::EthereumNetworkAdapters,
};
use graph::{
    blockchain::{
        Block, BlockHash, BlockPtr, Blockchain, IngestorAdapter as IngestorAdapterTrait,
        IngestorError,
    },
    log::factory::LoggerFactory,
    prelude::{
        async_trait, error, serde_json as json,
        slog::{o, Logger},
        web3::types::H256,
        BlockNumber, ChainStore, EthereumBlock, Future01CompatExt, LightEthereumBlock,
        LightEthereumBlockExt,
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
        block_hash: &BlockHash,
    ) -> Result<Option<BlockHash>, IngestorError> {
        let block_hash = H256::from_slice(block_hash.as_slice());

        let block = self
            .eth_adapter
            .block_by_hash(&self.logger, block_hash)
            .compat()
            .await?
            .ok_or_else(|| IngestorError::BlockUnavailable(block_hash))?;

        let ethereum_block = self
            .eth_adapter
            .load_full_block(&self.logger, block)
            .await?;
        let ethereum_block = BlockFinality::NonFinal(ethereum_block);

        // Store it in the database and try to advance the chain head pointer
        self.chain_store
            .upsert_block(Arc::new(ethereum_block))
            .await?;

        self.chain_store
            .clone()
            .attempt_chain_head_update(self.ancestor_count)
            .await
            .map(|missing| missing.map(|h256| h256.into()))
            .map_err(|e| {
                error!(self.logger, "failed to update chain head");
                IngestorError::Unknown(e)
            })
    }

    fn chain_head_ptr(&self) -> Result<Option<BlockPtr>, anyhow::Error> {
        self.chain_store.chain_head_ptr()
    }
}

/// This is used in `EthereumAdapter::triggers_in_block`, called when re-processing a block for
/// newly created data sources. This allows the re-processing to be reorg safe without having to
/// always fetch the full block data.
#[derive(Clone, Debug)]
pub enum BlockFinality {
    /// If a block is final, we only need the header and the triggers.
    Final(Arc<LightEthereumBlock>),

    // If a block may still be reorged, we need to work with more local data.
    NonFinal(EthereumBlock),
}

impl BlockFinality {
    pub(crate) fn light_block(&self) -> Arc<LightEthereumBlock> {
        match self {
            BlockFinality::Final(block) => block.clone(),
            BlockFinality::NonFinal(block) => block.block.clone(),
        }
    }
}

impl<'a> From<&'a BlockFinality> for BlockPtr {
    fn from(block: &'a BlockFinality) -> BlockPtr {
        match block {
            BlockFinality::Final(b) => BlockPtr::from(&**b),
            BlockFinality::NonFinal(b) => BlockPtr::from(b),
        }
    }
}

impl Block for BlockFinality {
    fn ptr(&self) -> BlockPtr {
        match self {
            BlockFinality::Final(block) => block.block_ptr(),
            BlockFinality::NonFinal(block) => block.block.block_ptr(),
        }
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        match self {
            BlockFinality::Final(block) => block.parent_ptr(),
            BlockFinality::NonFinal(block) => block.block.parent_ptr(),
        }
    }

    fn data(&self) -> Result<json::Value, json::Error> {
        match self {
            BlockFinality::Final(block) => {
                let eth_block = EthereumBlock {
                    block: block.clone(),
                    transaction_receipts: vec![],
                };
                json::to_value(eth_block)
            }
            BlockFinality::NonFinal(block) => json::to_value(&block.block),
        }
    }
}
