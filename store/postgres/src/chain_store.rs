use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::sql_types::Text;
use diesel::{insert_into, update};
use graph::blockchain::{Block, ChainIdentifier};
use graph::prelude::web3::types::H256;
use graph::{
    constraint_violation,
    prelude::{
        async_trait, ethabi, serde_json as json, BlockNumber, BlockPtr, CancelableError,
        ChainStore as ChainStoreTrait, Error, StoreError,
    },
};

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    iter::FromIterator,
    sync::Arc,
};

use crate::{block_store::ChainStatus, connection_pool::ConnectionPool};

pub use data::Storage;
mod data {
    use diesel::sql_types::Binary;
    use diesel::{connection::SimpleConnection, insert_into};
    use diesel::{delete, prelude::*, sql_query};
    use diesel::{dsl::sql, pg::PgConnection};
    use diesel::{
        pg::Pg,
        serialize::Output,
        sql_types::Text,
        types::{FromSql, ToSql},
    };
    use diesel::{
        sql_types::{BigInt, Bytea, Integer, Jsonb},
        update,
    };

    use graph::blockchain::{Block, BlockHash};
    use graph::{constraint_violation, prelude::StoreError};
    use std::fmt;
    use std::iter::FromIterator;
    use std::{convert::TryFrom, io::Write};

    #[derive(Clone, Debug, AsExpression, FromSqlRow)]
    #[sql_type = "diesel::sql_types::Text"]
    pub enum Storage {
        /// Chain data is stored in shared tables
        Shared,
    }

    impl fmt::Display for Storage {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match self {
                Self::Shared => Self::PUBLIC.fmt(f),
            }
        }
    }

    impl FromSql<Text, Pg> for Storage {
        fn from_sql(bytes: Option<&[u8]>) -> diesel::deserialize::Result<Self> {
            let s = <String as FromSql<Text, Pg>>::from_sql(bytes)?;
            Self::new(s).map_err(Into::into)
        }
    }

    impl ToSql<Text, Pg> for Storage {
        fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> diesel::serialize::Result {
            <String as ToSql<Text, Pg>>::to_sql(&self.to_string(), out)
        }
    }

    impl Storage {
        const PUBLIC: &'static str = "public";

        fn new(s: String) -> Result<Self, String> {
            Ok(Self::Shared)
        }
    }
}
pub struct ChainStore {
    pool: ConnectionPool,
    pub chain: String,
    genesis_block_ptr: BlockPtr,
    status: ChainStatus,
}

impl ChainStore {
    pub(crate) fn new(
        chain: String,
        net_identifier: &ChainIdentifier,
        status: ChainStatus,
        pool: ConnectionPool,
    ) -> Self {
        let store = ChainStore {
            pool,
            chain,
            genesis_block_ptr: BlockPtr::new(net_identifier.genesis_block_hash.clone(), 0),
            status,
        };
        store
    }

    pub fn is_ingestible(&self) -> bool {
        matches!(self.status, ChainStatus::Ingestible)
    }

    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, Error> {
        self.pool.get().map_err(Error::from)
    }

    pub(crate) fn create(&self, ident: &ChainIdentifier) -> Result<(), Error> {
        unimplemented!()
    }

    pub(crate) fn drop_chain(&self) -> Result<(), Error> {
        unimplemented!()
    }

    pub fn chain_head_pointers(
        conn: &PgConnection,
    ) -> Result<HashMap<String, BlockPtr>, StoreError> {
        unimplemented!()
    }

    pub fn chain_head_block(&self, chain: &str) -> Result<Option<BlockNumber>, StoreError> {
        unimplemented!()
    }

    /// Store the given chain as the blocks for the `network` set the
    /// network's genesis block to `genesis_hash`, and head block to
    /// `null`
    #[cfg(debug_assertions)]
    pub fn set_chain(&self, genesis_hash: &str, chain: Vec<&dyn Block>) {
        unimplemented!()
    }

    pub fn truncate_block_cache(&self) -> Result<(), StoreError> {
        unimplemented!()
    }
}

#[async_trait]
impl ChainStoreTrait for ChainStore {
    fn genesis_block_ptr(&self) -> Result<BlockPtr, Error> {
        Ok(self.genesis_block_ptr.clone())
    }

    async fn upsert_block(&self, block: Arc<dyn Block>) -> Result<(), Error> {
        unimplemented!();
    }

    fn upsert_light_blocks(&self, blocks: &[&dyn Block]) -> Result<(), Error> {
        unimplemented!();
    }

    async fn attempt_chain_head_update(
        self: Arc<Self>,
        ancestor_count: BlockNumber,
    ) -> Result<Option<H256>, Error> {
        unimplemented!()
    }

    fn chain_head_ptr(&self) -> Result<Option<BlockPtr>, Error> {
        unimplemented!()
    }

    fn blocks(&self, hashes: &[H256]) -> Result<Vec<json::Value>, Error> {
        unimplemented!();
    }

    fn ancestor_block(
        &self,
        block_ptr: BlockPtr,
        offset: BlockNumber,
    ) -> Result<Option<json::Value>, Error> {
        unimplemented!();
    }

    fn cleanup_cached_blocks(
        &self,
        ancestor_count: BlockNumber,
    ) -> Result<Option<(BlockNumber, usize)>, Error> {
        unimplemented!()
    }
}
