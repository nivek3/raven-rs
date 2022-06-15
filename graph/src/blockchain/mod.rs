pub mod block_ingestor;
pub mod block_stream;
pub mod block_types;

use crate::{components::store::BlockNumber, prelude::thiserror::Error};

use anyhow::Result;
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
}
pub trait TriggerData {
    /// If there is an error when processing this trigger, this will called to add relevant context.
    /// For example an useful return is: `"block #<N> (<hash>), transaction <tx_hash>".
    fn error_context(&self) -> String;
}

// pub trait DataSource<C: Blockchain>: 'static + Sized + Send + Sync + Clone + Send + Sync {
//     fn address(&self) -> Option<&[u8]>;
//     fn start_block(&self) -> BlockNumber;
//     fn name(&self) -> &str;
//     fn kind(&self) -> &str;
//     fn network(&self) -> Option<&str>;
//     fn creation_block(&self) -> Option<BlockNumber>;
//     fn runtime(&self) -> &[u8];
// }

// pub trait TriggerFilter<C: Blockchain>: Default + Clone + Send + Sync {
//     fn from_data_sources<'a>(
//         data_sources: impl Iterator<Item = &'a C::DataSource> + Clone,
//     ) -> Self {
//         let mut this = Self::default();
//         this.extend(data_sources);
//         this
//     }

//     fn extend<'a>(&mut self, data_sources: impl Iterator<Item = &'a C::DataSource> + Clone);
// }

pub trait Blockchain: Debug + Sized + Send + Sync + Unpin + 'static {
    type Block: Block + Clone;
    // type DataSource: DataSource<Self>;

    /// Trigger data as parsed from the triggers adapter.
    // type TriggerData: TriggerData + Ord;

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

impl From<anyhow::Error> for IngestorError {
    fn from(e: anyhow::Error) -> Self {
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

    /// Retrieve all necessary data for the block  `hash` from the chain and
    /// store it in the database
    async fn ingest_block(&self, hash: &BlockHash) -> Result<Option<BlockHash>, IngestorError>;

    /// Return the chain head that is stored locally, and therefore visible
    /// to the block streams of subgraphs
    fn chain_head_ptr(&self) -> Result<Option<BlockPtr>, anyhow::Error>;
}
