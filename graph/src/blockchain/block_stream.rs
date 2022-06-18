use anyhow::Error;
use futures03::Stream;

use super::{BlockPtr, Blockchain};
use crate::components::store::BlockNumber;
use crate::prelude::*;

pub trait BlockStream<C: Blockchain>:
    Stream<Item = Result<BlockStreamEvent, Error>> + Unpin
{
    fn notify_block_consumed(&mut self) {}
}

pub type FirehoseCursor = Option<String>;

// pub struct BlockWithTriggers<C: Blockchain> {
//     pub block: C::Block,
//     pub trigger_data: Vec<C::TriggerData>,
// }

// impl<C: Blockchain> BlockWithTriggers<C> {
//     pub fn new(block: C::Block, mut trigger_data: Vec<C::TriggerData>) -> Self {
//         // This is where triggers get sorted.
//         trigger_data.sort();
//         Self {
//             block,
//             trigger_data,
//         }
//     }

//     pub fn trigger_count(&self) -> usize {
//         self.trigger_data.len()
//     }

//     pub fn ptr(&self) -> BlockPtr {
//         self.block.ptr()
//     }
// }

#[async_trait]
pub trait TriggersAdapter<C: Blockchain>: Send + Sync {
    // Return the block that is `offset` blocks before the block pointed to
    // by `ptr` from the local cache. An offset of 0 means the block itself,
    // an offset of 1 means the block's parent etc. If the block is not in
    // the local cache, return `None`
    fn ancestor_block(&self, ptr: BlockPtr, offset: BlockNumber)
        -> Result<Option<C::Block>, Error>;

    /// Return `true` if the block with the given hash and number is on the
    /// main chain, i.e., the chain going back from the current chain head.
    async fn is_on_main_chain(&self, ptr: BlockPtr) -> Result<bool, Error>;

    /// Get pointer to parent of `block`. This is called when reverting `block`.
    async fn parent_ptr(&self, block: &BlockPtr) -> Result<Option<BlockPtr>, Error>;
}

pub enum BlockStreamEvent {
    // The payload is the current subgraph head pointer, which should be reverted, such that the
    // parent of the current subgraph head becomes the new subgraph head.
    // An optional pointer to the parent block will save a round trip operation when reverting.
    Revert(BlockPtr, FirehoseCursor, Option<BlockPtr>),
    // ProcessBlock(BlockWithTriggers<C>, FirehoseCursor),
}

/// Notifications about the chain head advancing. The block ingestor sends
/// an update on this stream whenever the head of the underlying chain
/// changes. The updates have no payload, receivers should call
/// `Store::chain_head_ptr` to check what the latest block is.
pub type ChainHeadUpdateStream = Box<dyn Stream<Item = ()> + Send + Unpin>;

pub trait ChainHeadUpdateListener: Send + Sync + 'static {
    /// Subscribe to chain head updates for the given network.
    fn subscribe(&self, network: String, logger: Logger) -> ChainHeadUpdateStream;
}
