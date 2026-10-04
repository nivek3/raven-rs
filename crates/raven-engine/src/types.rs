//! Chain-neutral values shared by sources, the canonical engine and stores.
//!
//! `Network` is immutable once an index starts and binds persisted state
//! to a chain identity and starting position.

use crate::PositionError;

/// Minimal block identity for continuity checks; application payload lives in a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteBlockHeader<H> {
    pub number: u64,
    pub hash: H,
    pub parent_hash: H,
}

impl<H: PartialEq> LiteBlockHeader<H> {
    /// Both height and parent identity must extend the predecessor.
    pub fn extends(&self, parent: &Self) -> bool {
        parent.number.checked_add(1) == Some(self.number) && self.parent_hash == parent.hash
    }
}

/// A complete, ordered set of requested updates for one exact block hash.
/// An empty update vector still represents a block that must be processed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockBatch<H, U> {
    pub header: LiteBlockHeader<H>,
    pub updates: Vec<U>,
}

/// A block identity consisting of its height and hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPtr<H> {
    pub number: u64,
    pub hash: H,
}

impl<H: Clone> From<&LiteBlockHeader<H>> for BlockPtr<H> {
    /// Copies a header's height and hash into a persisted block pointer.
    fn from(header: &LiteBlockHeader<H>) -> Self {
        Self {
            number: header.number,
            hash: header.hash.clone(),
        }
    }
}

/// Immutable starting point. Its parent was not executed by the indexer.
/// Absence of a parent is valid only for a genesis start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network<H, I> {
    chain_identity: I,
    start_block: u64,
    parent: Option<LiteBlockHeader<H>>,
}

impl<H, I> Network<H, I> {
    /// Validates and records the immutable start position for this chain.
    pub fn new(
        chain_identity: I,
        start_block: u64,
        parent: Option<LiteBlockHeader<H>>,
    ) -> Result<Self, PositionError> {
        match (start_block, parent.as_ref()) {
            (0, None) => {}
            (start, Some(header)) if header.number.checked_add(1) == Some(start) => {}
            _ => return Err(PositionError::InvalidStartBlock),
        }
        Ok(Self {
            chain_identity,
            start_block,
            parent,
        })
    }

    /// Returns the chain identity bound to persisted state.
    pub fn chain_identity(&self) -> &I {
        &self.chain_identity
    }

    /// Returns the first block height owned by this index.
    pub fn start_block(&self) -> u64 {
        self.start_block
    }

    /// Returns the unprocessed predecessor of the first indexed block, if any.
    pub fn parent(&self) -> Option<&LiteBlockHeader<H>> {
        self.parent.as_ref()
    }
}

impl<H: PartialEq, I> Network<H, I> {
    /// Local continuity only. The engine must separately verify canonical identity.
    pub fn accepts_first(&self, header: &LiteBlockHeader<H>) -> bool {
        header.number == self.start_block
            && self
                .parent
                .as_ref()
                .map_or(header.number == 0, |parent| header.extends(parent))
    }
}

/// Persisted state wins over configuration, even after every block is reverted.
/// This does not replace remote block pointer/indexing start verification before startup.
pub fn next_block_number<H, I>(
    start_block: u64,
    network: Option<&Network<H, I>>,
    block_ptr: Option<&BlockPtr<H>>,
) -> Result<u64, PositionError> {
    let next_block = match (network, block_ptr) {
        (None, Some(_)) => return Err(PositionError::MissingNetwork),
        (Some(network), Some(block_ptr)) => {
            if block_ptr.number < network.start_block {
                return Err(PositionError::BlockPtrBeforeStartBlock);
            }
            block_ptr
                .number
                .checked_add(1)
                .ok_or(PositionError::HeightOverflow)?
        }
        (Some(network), None) => network.start_block,
        (None, None) => start_block,
    };
    Ok(next_block)
}
