use async_trait::async_trait;
use std::sync::Arc;

use graph::{
    components::store::BlockStore as BlockStoreTrait,
    constraint_violation,
    prelude::{tokio, web3::types::Address, BlockPtr, StoreError},
};

use crate::block_store::BlockStore;

/// The overall store of the system, consisting of a [SubgraphStore] and a
/// [BlockStore], each of which multiplex across multiple database shards.
/// The `SubgraphStore` is responsible for storing all data and metadata related
/// to individual subgraphs, and the `BlockStore` does the same for data belonging
/// to the chains that are being processed.
///
/// This struct should only be used during configuration and setup of `graph-node`.
/// Code that needs to access the store should use the traits from [graph::components::store]
/// and only require the smallest traits that are suitable for their purpose
pub struct Store {
    block_store: Arc<BlockStore>,
}

impl Store {
    pub fn new(block_store: Arc<BlockStore>) -> Self {
        Self { block_store }
    }

    pub fn block_store(&self) -> Arc<BlockStore> {
        self.block_store.clone()
    }
}
