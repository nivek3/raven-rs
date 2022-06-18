use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
    sync::{Arc, RwLock},
};

use graph::{
    blockchain::ChainIdentifier,
    components::store::BlockStore as BlockStoreTrait,
    prelude::anyhow,
    prelude::{error, warn, BlockNumber, BlockPtr, Logger},
    prelude::{tokio, StoreError},
};

use crate::chain_store::Storage;
use crate::{connection_pool::ConnectionPool, ChainStore};

#[derive(Copy, Clone)]
pub enum ChainStatus {
    ReadOnly,
    Ingestible,
}

pub struct BlockStore {
    logger: Logger,
    /// Map chain names to the corresponding store. This map is updated
    /// dynamically with new chains if an operation would require a chain
    /// that is not yet in `stores`. It is initialized with all chains
    /// known to the system at startup, either from configuration or from
    /// previous state in the database.
    stores: RwLock<HashMap<String, Arc<ChainStore>>>,
    pool: ConnectionPool,
}

impl BlockStore {
    /// Create a new `BlockStore` by creating a `ChainStore` for each entry
    /// in `networks`. The creation process checks that the configuration for
    /// existing chains has not changed from the last time `graph-node` ran, and
    /// creates new entries in its chain directory for chains we had not used
    /// previously. It also creates a `ChainStore` for each chain that was used
    /// in previous runs of the node, regardless of whether it is mentioned in
    /// `chains` to ensure that queries against such chains will succeed.
    ///
    /// Each entry in `chains` gives the chain name, the network identifier,
    /// and the name of the database shard for the chain. The `ChainStore` for
    /// a chain uses the pool from `pools` for the given shard.
    pub fn new(
        logger: Logger,
        // (network, ident, shard)
        chains: Vec<(String, Vec<ChainIdentifier>)>,
        // shard -> pool
        pool: &ConnectionPool,
    ) -> Result<Self, StoreError> {
        let block_store = Self {
            logger,
            stores: RwLock::new(HashMap::new()),
            pool: pool.clone(),
        };

        fn reduce_idents(
            chain_name: &str,
            idents: Vec<ChainIdentifier>,
        ) -> Result<Option<ChainIdentifier>, StoreError> {
            let mut idents: HashSet<ChainIdentifier> = HashSet::from_iter(idents.into_iter());
            match idents.len() {
                0 => Ok(None),
                1 => Ok(idents.drain().next()),
                _ => Err(anyhow!(
                    "conflicting network identifiers for chain {}: {:?}",
                    chain_name,
                    idents
                )
                .into()),
            }
        }

        for (chain_name, idents) in chains {
            let ident = reduce_idents(&chain_name, idents)?;
            let storage = Storage::new();
            match ident {
                Some(ident) => {
                    block_store.add_chain_store(
                        chain_name,
                        &storage,
                        pool,
                        ident,
                        ChainStatus::Ingestible,
                    )?;
                }
                None => {
                    error!(
                        &block_store.logger,
                        " the chain {} is new but we could not get a network identifier for it",
                        chain_name
                    );
                }
            };
        }

        Ok(block_store)
    }

    fn add_chain_store(
        &self,
        chain_name: String,
        storage: &Storage,
        pool: &ConnectionPool,
        ident: ChainIdentifier,
        status: ChainStatus,
    ) -> Result<Arc<ChainStore>, StoreError> {
        let store = ChainStore::new(
            chain_name.clone().to_string(),
            storage.clone(),
            &ident.clone(), //
            status,
            pool.clone(),
        );
        let store = Arc::new(store);

        self.stores
            .write()
            .unwrap()
            .insert(chain_name.clone(), store.clone());
        Ok(store)
    }

    /// Return a map from network name to the network's chain head pointer.
    /// The information is cached briefly since this method is used heavily
    /// by the indexing status API
    pub fn chain_head_pointers(&self) -> Result<HashMap<String, BlockPtr>, StoreError> {
        unimplemented!()
    }

    pub fn chain_head_block(&self, chain: &str) -> Result<Option<BlockNumber>, StoreError> {
        unimplemented!()
    }

    fn store(&self, chain: &str) -> Option<Arc<ChainStore>> {
        let store = self.stores.read().unwrap();
        store.get(chain).map(|c| c.clone())
    }

    pub fn drop_chain(&self, chain: &str) -> Result<(), StoreError> {
        unimplemented!()
    }

    pub fn update_db_version(&self) -> Result<(), StoreError> {
        unimplemented!()
    }
}

impl BlockStoreTrait for BlockStore {
    type ChainStore = ChainStore;

    fn chain_store(&self, network: &str) -> Option<Arc<Self::ChainStore>> {
        self.store(network)
    }
}
