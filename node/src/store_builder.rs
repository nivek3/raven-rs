use std::iter::FromIterator;
use std::{collections::HashMap, sync::Arc};

use crate::config::Config;
use graph::blockchain::ChainIdentifier;
use graph::prelude::o;
use graph::prelude::{info, Logger};
use graph_store_postgres::connection_pool::{ConnectionPool, ForeignServer, PoolName};
use graph_store_postgres::{
    BlockStore as DieselBlockStore, ChainHeadUpdateListener as PostgresChainHeadUpdateListener,
    NotificationSender, Store as DieselStore, SubgraphStore,
    SubscriptionManager, PRIMARY_SHARD,
};

pub struct StoreBuilder {
    logger: Logger,
    subscription_manager: Arc<SubscriptionManager>,
    chain_head_update_listener: Arc<PostgresChainHeadUpdateListener>,
    /// Map network names to the shards where they are/should be stored
}

impl StoreBuilder {
    /// Set up all stores, and run migrations. This does a complete store
    /// setup whereas other methods here only get connections for an already
    /// initialized store
    pub async fn new(logger: &Logger, config: &Config) -> Self {
        let primary_shard = config.primary_store().clone();

        let subscription_manager = Arc::new(SubscriptionManager::new(
            logger.cheap_clone(),
            primary_shard.connection.to_owned(),
        ));

        let chain_head_update_listener = Arc::new(PostgresChainHeadUpdateListener::new(
            &logger,
            primary_shard.connection.to_owned(),
        ));

        Self {
            logger: logger.cheap_clone(),
            subscription_manager,
            chain_head_update_listener,
        }
    }

    /// Create a connection pool for the main database of the primary shard
    /// without connecting to all the other configured databases
    pub fn main_pool(
        logger: &Logger,
        name: &str,
        shard: &Shard,
        registry: Arc<dyn MetricsRegistry>,
        servers: Arc<Vec<ForeignServer>>,
    ) -> ConnectionPool {
        let logger = logger.new(o!("pool" => "main"));
        let pool_size = shard.pool_size.size_for(node, name).expect(&format!(
            "we can determine the pool size for store {}",
            name
        ));
        let fdw_pool_size = shard.fdw_pool_size.size_for(node, name).expect(&format!(
            "we can determine the fdw pool size for store {}",
            name
        ));
        info!(
            logger,
            "Connecting to Postgres";
            "url" => SafeDisplay(shard.connection.as_str()),
            "conn_pool_size" => pool_size,
            "weight" => shard.weight
        );
        ConnectionPool::create(
            name,
            PoolName::Main,
            shard.connection.to_owned(),
            pool_size,
            Some(fdw_pool_size),
            &logger,
            registry.cheap_clone(),
            servers,
        )
    }

    pub fn subscription_manager(&self) -> Arc<SubscriptionManager> {
        self.subscription_manager.cheap_clone()
    }

    pub fn chain_head_update_listener(&self) -> Arc<PostgresChainHeadUpdateListener> {
        self.chain_head_update_listener.clone()
    }

    pub fn primary_pool(&self) -> ConnectionPool {
        self.pools.get(&*PRIMARY_SHARD).unwrap().clone()
    }
}
