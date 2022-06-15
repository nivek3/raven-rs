use std::sync::Arc;
use thiserror::Error;
use web3::types::H256;

use crate::{
    blockchain::Block,
    prelude::{anyhow::Error, async_trait, BlockPtr},
};

pub type BlockNumber = i32;

pub trait BlockStore: Send + Sync + 'static {
    type ChainStore: ChainStore;

    fn chain_store(&self, network: &str) -> Option<Arc<Self::ChainStore>>;
}

/// Common trait for blockchain store implementations.
#[async_trait]
pub trait ChainStore: Send + Sync + 'static {
    /// Get a pointer to this blockchain's genesis block.
    fn genesis_block_ptr(&self) -> Result<BlockPtr, Error>;

    /// Insert a block into the store (or update if they are already present).
    async fn upsert_block(&self, block: Arc<dyn Block>) -> Result<(), Error>;

    /// Upsert a block into the store (or update if they are already present).
    fn upsert_light_blocks(&self, blocks: &[&dyn Block]) -> Result<(), Error>;

    async fn attempt_chain_head_update(
        self: Arc<Self>,
        ancestor_count: BlockNumber,
    ) -> Result<Option<H256>, Error>;

    /// Get the current head block pointer for this chain.
    /// Any changes to the head block pointer will be to a block with a larger block number, never
    /// to a block with a smaller or equal block number.
    ///
    /// The head block pointer will be None on initial set up.
    fn chain_head_ptr(&self) -> Result<Option<BlockPtr>, Error>;

    /// Returns the blocks present in the store.
    fn blocks(&self, hashes: &[H256]) -> Result<Vec<serde_json::Value>, Error>;

    /// Get the `offset`th ancestor of `block_hash`, where offset=0 means the block matching
    /// `block_hash` and offset=1 means its parent. Returns None if unable to complete due to
    /// missing blocks in the chain store.
    ///
    /// Returns an error if the offset would reach past the genesis block.
    fn ancestor_block(
        &self,
        block_ptr: BlockPtr,
        offset: BlockNumber,
    ) -> Result<Option<serde_json::Value>, Error>;

    /// Remove old blocks from the cache we maintain in the database and
    /// return a pair containing the number of the oldest block retained
    /// and the number of blocks deleted.
    /// We will never remove blocks that are within `ancestor_count` of
    /// the chain head.
    fn cleanup_cached_blocks(
        &self,
        ancestor_count: BlockNumber,
    ) -> Result<Option<(BlockNumber, usize)>, Error>;
}

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("store error: {0}")]
    Unknown(Error),
    #[error(
        "tried to set entity of type `{0}` with ID \"{1}\" but an entity of type `{2}`, \
         which has an interface in common with `{0}`, exists with the same ID"
    )]
    ConflictingId(String, String, String), // (entity, id, conflicting_entity)
    #[error("unknown field '{0}'")]
    UnknownField(String),
    #[error("unknown table '{0}'")]
    UnknownTable(String),
    #[error("malformed directive '{0}'")]
    MalformedDirective(String),
    #[error("query execution failed: {0}")]
    QueryExecutionError(String),
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(String),
    /// An internal error where we expected the application logic to enforce
    /// some constraint, e.g., that subgraph names are unique, but found that
    /// constraint to not hold
    #[error("internal constraint violated: {0}")]
    ConstraintViolation(String),
    #[error("deployment not found: {0}")]
    DeploymentNotFound(String),
    #[error("shard not found: {0} (this usually indicates a misconfiguration)")]
    UnknownShard(String),
    #[error("Fulltext search not yet deterministic")]
    FullTextSearchNonDeterministic,
    #[error("operation was canceled")]
    Canceled,
    #[error("database unavailable")]
    DatabaseUnavailable,
}

// Convenience to report a constraint violation
#[macro_export]
macro_rules! constraint_violation {
    ($msg:expr) => {{
        StoreError::ConstraintViolation(format!("{}", $msg))
    }};
    ($fmt:expr, $($arg:tt)*) => {{
        StoreError::ConstraintViolation(format!($fmt, $($arg)*))
    }}
}

impl From<::diesel::result::Error> for StoreError {
    fn from(e: ::diesel::result::Error) -> Self {
        StoreError::Unknown(e.into())
    }
}

impl From<::diesel::r2d2::PoolError> for StoreError {
    fn from(e: ::diesel::r2d2::PoolError) -> Self {
        StoreError::Unknown(e.into())
    }
}

impl From<Error> for StoreError {
    fn from(e: Error) -> Self {
        StoreError::Unknown(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Unknown(e.into())
    }
}
