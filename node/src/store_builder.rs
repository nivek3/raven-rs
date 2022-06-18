use std::sync::Arc;

use graph::{
    blockchain::ChainIdentifier,
    prelude::{
        o, {info, Logger},
    },
    util::security::SafeDisplay,
};

use crate::config::{Config, Shard};
use graph_store_postgres::{
    connection_pool::{ConnectionPool, PoolName},
    BlockStore as DieselBlockStore, Store as DieselStore,
};

pub struct StoreBuilder {
    pub logger: Logger,
    pub pool: ConnectionPool,
}

impl StoreBuilder {
    /// Set up all stores, and run migrations. This does a complete store
    /// setup whereas other methods here only get connections for an already
    /// initialized store
    pub async fn new(logger: &Logger, config: &Config) -> Self {
        let pool = Self::make_pg_pool(logger, config);
        pool.setup().await;

        Self {
            logger: logger.clone(),
            pool,
        }
    }

    pub fn make_pg_pool(logger: &Logger, config: &Config) -> ConnectionPool {
        let name = "primary";
        let shard = config.shard.clone();
        let logger = logger.new(o!("shard" => name.clone().to_string()));
        let conn_pool = Self::main_pool(&logger, name, &shard);
        conn_pool
    }

    /// Create a connection pool for the main database of the primary shard
    /// without connecting to all the other configured databases
    pub fn main_pool(logger: &Logger, name: &str, shard: &Shard) -> ConnectionPool {
        let logger = logger.new(o!("pool" => "main"));
        let pool_size: u32 = 10;
        let fdw_pool_size: u32 = 5;
        info!(
            logger,
            "Connecting to Postgres";
            "url" => SafeDisplay(shard.connection.as_str()),
            "conn_pool_size" => pool_size,
        );
        ConnectionPool::create(
            name,
            PoolName::Main,
            shard.connection.to_owned(),
            pool_size,
            Some(fdw_pool_size),
            &logger,
        )
    }

    pub fn network_store(self, networks: Vec<(String, Vec<ChainIdentifier>)>) -> Arc<DieselStore> {
        Self::make_store(&self.logger, &self.pool, networks)
    }

    pub fn make_store(
        logger: &Logger,
        pool: &ConnectionPool,
        networks: Vec<(String, Vec<ChainIdentifier>)>,
    ) -> Arc<DieselStore> {
        let logger = logger.new(o!("component" => "BlockStore"));

        let block_store = Arc::new(
            DieselBlockStore::new(logger, networks, &pool).expect("Creating the BlockStore works"),
        );

        Arc::new(DieselStore::new(block_store))
    }
}
