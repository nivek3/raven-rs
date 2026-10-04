//! Canonical state machine for applying, reverting and resynchronizing block batches.

use std::sync::Arc;

use crate::{
    BlockBatch, BlockProcessor, BlockPtr, BlockSource, ChainStore, EngineError, FinalityPolicy,
    LiteBlockHeader, Network, PositionError, RavenResult, entity::EntityState, next_block_number,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChainUpdate<H, U> {
    Apply(BlockBatch<H, U>),
    Revert(LiteBlockHeader<H>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestOutcome {
    Applied,
    Ignored,
    Resync,
    /// The block may become eligible later; keep the committed block pointer unchanged.
    Deferred,
}

/// Indexing engine with a configurable processing delay. Preparation does not mutate
/// indexed state; replacement batches and rollback headers are verified before the first write.
pub struct Engine<S, T, P> {
    pub(crate) source: Arc<S>,
    pub(crate) store: T,
    pub(crate) processor: P,
    policy: FinalityPolicy,
}

impl<S, T, P> Engine<S, T, P>
where
    S: BlockSource,
    S::Hash: Clone + Eq,
    T: ChainStore<Hash = S::Hash, ChainIdentity = S::ChainIdentity>,
    P: BlockProcessor<S::Hash, S::Update>,
{
    /// Creates an engine that processes the supplied source through the given store and processor.
    pub fn new(source: Arc<S>, store: T, processor: P) -> Self {
        Self {
            source,
            store,
            processor,
            policy: FinalityPolicy::Head,
        }
    }

    /// Configure before running. Existing canonical state is retained even when a
    /// stricter delay makes its block pointer higher than the new processable height.
    pub fn with_finality_policy(mut self, policy: FinalityPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Uses the persisted network metadata after initialization, even when block pointer is None.
    pub async fn next_block_number(&self, start_block: u64) -> RavenResult<u64> {
        let network = self.store.network().await?;
        let block_ptr = self.store.block_ptr().await?;
        Ok(next_block_number(
            start_block,
            network.as_ref(),
            block_ptr.as_ref(),
        )?)
    }

    /// Initialize the network metadata once. Caller must resync before consuming a stream.
    pub async fn initialize(&mut self, start_block: u64) -> RavenResult<()> {
        let identity = self.source.chain_identity().await?;
        if let Some(network) = self.store.network().await? {
            if network.chain_identity() != &identity {
                return Err(EngineError::ChainIdentityMismatch.into());
            }
            self.next_block_number(start_block).await?;
            return Ok(());
        }
        if self.store.block_ptr().await?.is_some() {
            return Err(PositionError::MissingNetwork.into());
        }
        let head = self.source.head().await?;
        if !self.policy.permits(start_block, head.number) {
            return Err(EngineError::SourceBehind.into());
        }
        tracing::info!(
            start_block = start_block,
            head = head.number,
            "Preparing network indexing start"
        );
        let first = self
            .source
            .header_by_number(start_block)
            .await?
            .ok_or(EngineError::MissingBlock)?;
        if first.number != start_block {
            return Err(EngineError::InvalidBlock.into());
        }
        let parent = if start_block == 0 {
            None
        } else {
            let parent = self
                .source
                .header_by_hash(&first.parent_hash)
                .await?
                .ok_or(EngineError::MissingBlock)?;
            if parent.hash != first.parent_hash || !first.extends(&parent) {
                return Err(EngineError::InvalidBlock.into());
            }
            Some(parent)
        };
        self.verify_canonical(&first).await?;
        self.verify_prepared_head(&head, Some(start_block)).await?;
        let network = Network::new(identity, start_block, parent)?;
        self.store.initialize(&network).await?;
        tracing::info!(
            start_block = start_block,
            "Network indexing start persisted"
        );
        Ok(())
    }

    /// A canonical committed position can resume streaming without downloading
    /// the entire remote suffix. No state changes occur here. A mismatch goes
    /// through full resynchronization after the producer is cancelled and joined.
    pub(crate) async fn stream_is_current(&self) -> RavenResult<bool> {
        let network = self
            .store
            .network()
            .await?
            .ok_or(PositionError::MissingNetwork)?;
        if self.source.chain_identity().await? != *network.chain_identity() {
            return Err(EngineError::ChainIdentityMismatch.into());
        }
        let block_ptr = self.store.block_ptr().await?;
        next_block_number(network.start_block(), Some(&network), block_ptr.as_ref())?;
        let head = self.source.head().await?;
        if head.number < network.start_block() {
            return Err(EngineError::SourceBehind.into());
        }
        let position = match &block_ptr {
            Some(cp) => Some(self.local_header(cp).await?),
            None => network.parent().cloned(),
        };
        if let Some(position) = position {
            if position.number > head.number {
                return Ok(false);
            }
            let remote = self
                .source
                .header_by_number(position.number)
                .await?
                .ok_or(EngineError::MissingBlock)?;
            if remote.number != position.number {
                return Err(EngineError::InvalidBlock.into());
            }
            if remote.hash != position.hash {
                return Ok(false);
            }
            if remote != position {
                return Err(EngineError::InvalidBlock.into());
            }
        }
        self.verify_prepared_head(&head, None).await?;
        Ok(true)
    }

    /// Incoming batches never choose the canonical branch. An exact committed
    /// duplicate is ignored; divergence or a gap asks the runner to stop its producer.
    pub async fn ingest(
        &mut self,
        batch: BlockBatch<S::Hash, S::Update>,
    ) -> RavenResult<IngestOutcome> {
        let network = self
            .store
            .network()
            .await?
            .ok_or(PositionError::MissingNetwork)?;
        let block_ptr = self.store.block_ptr().await?;
        if let Some(block_ptr) = &block_ptr
            && batch.header.number <= block_ptr.number
        {
            let local = self
                .store
                .block_header_by_number(batch.header.number)
                .await?;
            return Ok(if local.as_ref() == Some(&batch.header) {
                IngestOutcome::Ignored
            } else {
                IngestOutcome::Resync
            });
        }
        let head = self.source.head().await?;
        if !self.policy.permits(batch.header.number, head.number) {
            return Ok(IngestOutcome::Deferred);
        }
        let adjacent = match &block_ptr {
            Some(block_ptr) => {
                let parent = self.local_header(block_ptr).await?;
                batch.header.extends(&parent)
            }
            None => network.accepts_first(&batch.header),
        };
        if !adjacent {
            return Ok(IngestOutcome::Resync);
        }
        // This adjacent block must match the current canonical observation and
        // the committed parent. Gap/reorg paths still walk one hash-anchored branch.
        let canonical = self
            .source
            .header_by_number(batch.header.number)
            .await?
            .ok_or(EngineError::MissingBlock)?;
        if canonical != batch.header {
            return Ok(IngestOutcome::Resync);
        }
        self.verify_canonical(&batch.header).await?;
        self.verify_prepared_head(&head, Some(batch.header.number))
            .await?;
        self.apply(block_ptr.as_ref(), &batch).await?;
        Ok(IngestOutcome::Applied)
    }

    /// A moving or unavailable branch returns an error before mutation. A failure
    /// during execution leaves an atomic committed prefix for the next invocation.
    pub async fn resync(&mut self) -> RavenResult<()> {
        let network = self
            .store
            .network()
            .await?
            .ok_or(PositionError::MissingNetwork)?;
        if self.source.chain_identity().await? != *network.chain_identity() {
            return Err(EngineError::ChainIdentityMismatch.into());
        }
        let block_ptr = self.store.block_ptr().await?;
        next_block_number(network.start_block(), Some(&network), block_ptr.as_ref())?;
        if let Some(block_ptr) = &block_ptr {
            self.local_header(block_ptr).await?;
        }
        let head = self.source.head().await?;
        if head.number < network.start_block() {
            return Err(EngineError::SourceBehind.into());
        }
        tracing::info!(
            head = head.number,
            block_ptr = block_ptr.as_ref().map(|cp| cp.number),
            "Preparing gap or reorg resynchronization"
        );
        let mut remote = self.exact(&head.hash).await?;
        if remote.header != head {
            return Err(EngineError::InvalidBlock.into());
        }
        let mut replacements = Vec::new();
        let ancestor = loop {
            let header = &remote.header;
            if header.number < network.start_block() {
                if network.parent() != Some(header) {
                    return Err(EngineError::ReorgBeforeStartBlock.into());
                }
                break None;
            }
            if let Some(local) = self.store.block_header_by_number(header.number).await?
                && local.hash == header.hash
            {
                if local != *header {
                    return Err(EngineError::InvalidLocalHistory.into());
                }
                // A lower remote head on the same local branch may be a lagging
                // endpoint. Do not remove verified descendants on that evidence.
                if block_ptr.as_ref().is_some_and(|cp| cp.number > head.number)
                    && header.number == head.number
                {
                    return Err(EngineError::SourceBehind.into());
                }
                break Some(BlockPtr::from(header));
            }
            let current = remote.header.clone();
            // Walk the entire head branch to detect orphaned block pointers, even above
            // the processing limit. Only eligible replacements reach execution.
            if self.policy.permits(current.number, head.number) {
                replacements.push(remote);
            }
            if current.number == 0 {
                if network.start_block() != 0 {
                    return Err(EngineError::ReorgBeforeStartBlock.into());
                }
                break None;
            }
            remote = self.exact(&current.parent_hash).await?;
            if !current.extends(&remote.header) {
                return Err(EngineError::InvalidBlock.into());
            }
        };

        // Verify the entire local rollback path before reverting even one block.
        let mut transitions = Vec::new();
        let mut local = block_ptr.clone();
        while local != ancestor {
            let pointer = local.as_ref().ok_or(EngineError::InvalidLocalHistory)?;
            let header = self.local_header(pointer).await?;
            let parent = if header.number == network.start_block() {
                if !network.accepts_first(&header) {
                    return Err(EngineError::InvalidLocalHistory.into());
                }
                None
            } else {
                let parent = self
                    .store
                    .block_header_by_number(header.number - 1)
                    .await?
                    .ok_or(EngineError::InvalidLocalHistory)?;
                if !header.extends(&parent) {
                    return Err(EngineError::InvalidLocalHistory.into());
                }
                Some(BlockPtr::from(&parent))
            };
            transitions.push(ChainUpdate::Revert(header));
            local = parent;
        }
        let max_apply = replacements.first().map(|batch| batch.header.number);
        transitions.extend(replacements.into_iter().rev().map(ChainUpdate::Apply));
        self.verify_prepared_head(&head, max_apply).await?;
        if self.store.block_ptr().await? != block_ptr {
            return Err(EngineError::BlockPtrConflict.into());
        }
        let mut expected = block_ptr;
        for transition in transitions {
            match transition {
                ChainUpdate::Revert(header) => {
                    self.store.revert_block(&header).await?;
                    tracing::info!(block = header.number, "Block reverted");
                    expected = self.store.block_ptr().await?;
                }
                ChainUpdate::Apply(batch) => {
                    self.apply(expected.as_ref(), &batch).await?;
                    expected = Some(BlockPtr::from(&batch.header));
                }
            }
        }
        // The remote chain can change after preflight; the next retry starts from
        // the block pointer that was actually committed, never a prepared stream position.
        self.verify_prepared_head(&head, max_apply).await
    }

    /// Confirms that the prepared head remains canonical and supports the planned applied height.
    async fn verify_prepared_head(
        &self,
        head: &LiteBlockHeader<S::Hash>,
        max_apply: Option<u64>,
    ) -> RavenResult<()> {
        self.verify_canonical(head).await?;
        let current = self.source.head().await?;
        if current.number < head.number
            || (current.number == head.number && current.hash != head.hash)
            || max_apply.is_some_and(|number| !self.policy.permits(number, current.number))
        {
            return Err(EngineError::SourceChanged.into());
        }
        Ok(())
    }

    /// Loads the batch for exactly the requested hash and rejects substitutions.
    async fn exact(&self, hash: &S::Hash) -> RavenResult<BlockBatch<S::Hash, S::Update>> {
        let batch = self
            .source
            .block_by_hash(hash)
            .await?
            .ok_or(EngineError::MissingBlock)?;
        if &batch.header.hash != hash {
            return Err(EngineError::InvalidBlock.into());
        }
        Ok(batch)
    }

    /// Confirms that a header still matches the canonical observation at its height.
    async fn verify_canonical(&self, header: &LiteBlockHeader<S::Hash>) -> RavenResult<()> {
        let current = self
            .source
            .header_by_number(header.number)
            .await?
            .ok_or(EngineError::SourceChanged)?;
        if current != *header {
            return Err(EngineError::SourceChanged.into());
        }
        Ok(())
    }

    /// Loads a locally committed header and checks that it matches its stored pointer.
    async fn local_header(
        &self,
        block_ptr: &BlockPtr<S::Hash>,
    ) -> RavenResult<LiteBlockHeader<S::Hash>> {
        let header = self
            .store
            .block_header_by_number(block_ptr.number)
            .await?
            .ok_or(EngineError::InvalidLocalHistory)?;
        if header.number != block_ptr.number || header.hash != block_ptr.hash {
            return Err(EngineError::InvalidLocalHistory.into());
        }
        Ok(header)
    }

    /// Runs handlers against block-local state and atomically commits their resulting changes.
    async fn apply(
        &self,
        expected: Option<&BlockPtr<S::Hash>>,
        batch: &BlockBatch<S::Hash, S::Update>,
    ) -> RavenResult<()> {
        let mut entities = EntityState::new(&self.store, expected.cloned());
        self.processor.process(&mut entities, batch).await?;
        let changes = entities.into_changes()?;
        self.store
            .commit_block(expected, &batch.header, changes)
            .await?;
        tracing::info!(
            block = batch.header.number,
            updates = batch.updates.len(),
            "Block committed"
        );
        Ok(())
    }
}
