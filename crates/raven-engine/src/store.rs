//! Transactional persistence contracts for canonical blocks and application state.

use async_trait::async_trait;

use crate::{
    BlockBatch, BlockPtr, EntityChange, EntityStore, EntityValue, LiteBlockHeader, Network,
    RavenResult,
};

/// Runs application work against a block-scoped entity state before the write
/// transaction starts. Processors must not publish external side effects.
#[async_trait]
pub trait BlockProcessor<H: Send + Sync, U: Send + Sync>: Send + Sync {
    /// Processes every update in a batch against its block-local entity state.
    async fn process(
        &self,
        entities: &mut dyn EntityStore,
        batch: &BlockBatch<H, U>,
    ) -> RavenResult<()>;
}

/// A store session must hold exclusive writer ownership for its entire lifetime.
/// Implementations must reject writes if ownership is lost. The engine additionally
/// supplies expected block pointer identities to protect each apply transaction.
#[async_trait]
pub trait ChainStore: Send + Sync {
    type Hash: Clone + Eq + Send + Sync;
    type ChainIdentity: Clone + Eq + Send + Sync;

    /// Immutable chain identity and indexing start metadata.
    async fn network(&self) -> RavenResult<Option<Network<Self::Hash, Self::ChainIdentity>>>;

    /// Initialize empty state atomically. An existing, different network metadata is an error;
    /// initialization must never erase entities or change an existing block pointer.
    async fn initialize(
        &self,
        network: &Network<Self::Hash, Self::ChainIdentity>,
    ) -> RavenResult<()>;

    /// Last atomically committed indexing block, or None before any block is committed.
    async fn block_ptr(&self) -> RavenResult<Option<BlockPtr<Self::Hash>>>;

    /// Returns the header at this height on the locally indexed canonical chain.
    /// Orphaned blocks are excluded; this does not query the source's current head.
    async fn block_header_by_number(
        &self,
        number: u64,
    ) -> RavenResult<Option<LiteBlockHeader<Self::Hash>>>;

    /// Read committed entity state without holding a transaction across handler
    /// execution. Reject a changed block pointer before returning the value.
    async fn get_entity(
        &self,
        expected: Option<&BlockPtr<Self::Hash>>,
        entity_type: &str,
        id: &str,
    ) -> RavenResult<Option<EntityValue>>;

    /// Atomically validate the expected block pointer and continuity, then publish
    /// application entity versions, the block header and indexing progress.
    /// Errors or cancellation before commit leave the block unapplied.
    async fn commit_block(
        &self,
        expected: Option<&BlockPtr<Self::Hash>>,
        header: &LiteBlockHeader<Self::Hash>,
        changes: Vec<EntityChange>,
    ) -> RavenResult<()>;

    /// Target must be exactly the committed head. Restore the full previous entity
    /// state, canonical metadata and block pointer in one transaction. Reverting
    /// the first applied block clears the block pointer but retains the network metadata.
    async fn revert_block(&self, header: &LiteBlockHeader<Self::Hash>) -> RavenResult<()>;
}
