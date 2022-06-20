pub mod block_ingestor;
pub mod block_stream;
pub mod block_types;

use crate::{components::store::BlockNumber, prelude::thiserror::Error};

use anyhow::Error;
use async_trait::async_trait;
pub use block_types::{BlockHash, BlockPtr, ChainIdentifier};
use slog::Logger;
use std::{fmt::Debug, sync::Arc};
use web3::types::H256;

pub trait Block: Send + Sync {
    fn ptr(&self) -> BlockPtr;

    fn parent_ptr(&self) -> Option<BlockPtr>;

    fn number(&self) -> i32 {
        self.ptr().number
    }

    fn hash(&self) -> BlockHash {
        self.ptr().hash
    }

    fn parent_hash(&self) -> Option<BlockHash> {
        self.parent_ptr().map(|ptr| ptr.hash)
    }

    /// The data that should be stored for this block in the `ChainStore`
    fn data(&self) -> Result<serde_json::Value, serde_json::Error> {
        Ok(serde_json::Value::Null)
    }
}

pub trait Blockchain: Debug + Sized + Send + Sync + Unpin + 'static {
    type Block: Block + Clone;
    // type DataSource: DataSource<Self>;

    /// Trigger filter used as input to the triggers adapter.
    // type TriggerFilter: TriggerFilter<Self>;

    type IngestorAdapter: IngestorAdapter<Self>;

    fn ingestor_adapter(&self) -> Arc<Self::IngestorAdapter>;
}

#[derive(Debug, Error)]
pub enum IngestorError {
    #[error("Block data unavailable, block was likely uncle (block hash = {0:?})")]
    BlockUnavailable(H256),

    #[error("Receipt for tx {1:?} unavailable, block was likely uncle (block hash = {0:?})")]
    ReceiptUnavailable(H256, H256),

    /// An unexpected error occurred.
    #[error("Ingestor error: {0}")]
    Unknown(anyhow::Error),
}

impl From<Error> for IngestorError {
    fn from(e: Error) -> Self {
        IngestorError::Unknown(e)
    }
}

impl From<web3::Error> for IngestorError {
    fn from(e: web3::Error) -> Self {
        IngestorError::Unknown(anyhow::anyhow!(e))
    }
}

#[async_trait]
pub trait IngestorAdapter<C: Blockchain> {
    fn logger(&self) -> &Logger;

    /// How many ancestors of the current chain head to ingest. For chains
    /// that can experience reorgs, this should be large enough to cover all
    /// blocks that could be subject to reorgs to ensure that `graph-node`
    /// has enough blocks in its local cache to traverse a sidechain back to
    /// the main chain even if those blocks get removed from the network
    /// client.
    fn ancestor_count(&self) -> BlockNumber;

    /// Get the latest block from the chain
    async fn latest_block(&self) -> Result<BlockPtr, IngestorError>;

    /// Retrieve all necessary data for the block `hash` from the chain and
    /// store it in the database, return the next block number to ingest.
    async fn ingest_block(&self, hash: &BlockHash) -> Result<Option<BlockNumber>, IngestorError>;

    /// Return the chain head that is stored locally, and therefore visible
    /// to the block streams of subgraphs
    fn chain_head_ptr(&self) -> Result<Option<BlockPtr>, anyhow::Error>;

    async fn chain_block_ptr(&self, block_number: BlockNumber) -> Result<BlockPtr, IngestorError>;
}
