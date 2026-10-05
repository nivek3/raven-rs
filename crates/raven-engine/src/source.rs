//! Sequential and random-access source contracts used by the canonical pipeline.

use async_trait::async_trait;

pub use tokio::sync::mpsc::Sender;
pub use tokio_util::sync::CancellationToken;

use crate::{BlockBatch, LiteBlockHeader, RavenResult};

/// Sequential production; the pipeline derives the next block height from committed state.
/// Sources own payload construction: updates must belong to their batch header,
/// satisfy the configured payload requirements and filters, and be unique and
/// ordered for processing. The engine trusts this contract and checks chain headers.
#[async_trait]
pub trait Datasource: Send + Sync {
    type Hash: Send + Sync;
    type Update: Send + Sync;

    /// Emit complete ordered batches ready for processing, including empty blocks,
    /// starting at `next_block` and satisfying the source's configured requirements.
    /// Cancellation must interrupt blocked sends and acquisition work. Once cancelled,
    /// return promptly; the pipeline joins the producer and discards its queue.
    /// Tasks spawned by this method must be cancelled or aborted if its future is dropped.
    /// Finite sources return `Ok(())` at exhaustion; errors must not become empty batches.
    /// SourceChanged, SourceBehind and MissingBlock engine errors trigger delayed
    /// resynchronization and a new producer from committed progress. Other errors
    /// terminate the pipeline; unavailable payloads must never be emitted as empty blocks.
    async fn consume(
        &self,
        next_block: u64,
        sender: Sender<BlockBatch<Self::Hash, Self::Update>>,
        cancellation: CancellationToken,
    ) -> RavenResult<()>;
}

/// Random access to a configured chain with immutable data requirements for a run.
/// Paired source paths must return equivalent batches for the same block hash.
/// Direct Engine callers must configure paired sources themselves.
/// Returned payloads must match their batch header and satisfy the configured
/// requirements and filters, with unique updates ordered for processing. Sources
/// own payload identity, completeness and ordering; the engine trusts this contract
/// and checks chain headers. Unavailability is not proof of a reorg.
#[async_trait]
pub trait BlockSource: Send + Sync {
    type Hash: Send + Sync;
    type Update: Send + Sync;
    type ChainIdentity: Clone + Eq + Send + Sync;

    /// Returns the stable identity of the configured chain.
    async fn chain_identity(&self) -> RavenResult<Self::ChainIdentity>;

    /// Returns the source's current canonical head observation.
    async fn head(&self) -> RavenResult<LiteBlockHeader<Self::Hash>>;

    /// Lightweight canonical header lookup. It is a point-in-time observation;
    /// callers must recheck canonicality before using it to commit. The fallback
    /// extracts the header from the configured block source.
    async fn header_by_number(
        &self,
        number: u64,
    ) -> RavenResult<Option<LiteBlockHeader<Self::Hash>>> {
        let batch = self.block_by_number(number).await?;
        Ok(batch.map(|batch| batch.header))
    }

    /// Exact-hash header lookup, including retained orphans. Returning a different
    /// hash is invalid. Implementations may omit transactions and logs here.
    async fn header_by_hash(
        &self,
        hash: &Self::Hash,
    ) -> RavenResult<Option<LiteBlockHeader<Self::Hash>>> {
        let batch = self.block_by_hash(hash).await?;
        Ok(batch.map(|batch| batch.header))
    }

    /// Canonical lookup at request time, not a snapshot shared with other requests.
    async fn block_by_number(
        &self,
        number: u64,
    ) -> RavenResult<Option<BlockBatch<Self::Hash, Self::Update>>>;

    /// Return the exact requested hash with all configured payload, even for an orphan
    /// if retained by the source. Never substitute the current block at that height.
    async fn block_by_hash(
        &self,
        hash: &Self::Hash,
    ) -> RavenResult<Option<BlockBatch<Self::Hash, Self::Update>>>;
}
