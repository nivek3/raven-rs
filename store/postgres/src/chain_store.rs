use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, PooledConnection};

use diesel::prelude::*;
use graph::blockchain::{Block, ChainIdentifier};
use graph::prelude::web3::types::H256;
use graph::prelude::{
    async_trait, serde_json as json, BlockNumber, BlockPtr, CancelableError,
    ChainStore as ChainStoreTrait, Error, StoreError,
};

use std::{collections::HashMap, convert::TryFrom, sync::Arc};

use crate::{block_store::ChainStatus, connection_pool::ConnectionPool};

pub use data::Storage;
mod data {
    use diesel::insert_into;
    use diesel::pg::PgConnection;
    use diesel::sql_types::{BigInt, Bytea, Integer, Jsonb};
    use diesel::{prelude::*, sql_query, sql_types::Text};

    use graph::blockchain::{Block, BlockHash};
    use graph::prelude::{
        serde_json as json, web3::types::H256, BlockNumber, BlockPtr, Error, StoreError,
    };
    use std::convert::TryFrom;
    use std::fmt;

    #[derive(QueryableByName)]
    struct BlockHashText {
        #[sql_type = "Text"]
        hash: String,
    }

    #[derive(Clone, Debug, AsExpression)]

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

    impl Storage {
        const PUBLIC: &'static str = "public";

        pub fn new() -> Self {
            Self::Shared
        }

        pub(super) fn upsert_block(
            &self,
            conn: &PgConnection,
            chain: &str,
            block: &dyn Block,
            _overwrite: bool,
        ) -> Result<(), StoreError> {
            use crate::models::block::ethereum_blocks as b;
            const NO_PARENT: &str =
                "0000000000000000000000000000000000000000000000000000000000000000";
            let number = block.number() as i64;
            let hash = block.hash();
            let data = block.data().expect("Failed to serialize block");
            let parent_hash = block.parent_hash().unwrap_or_else(|| {
                BlockHash::try_from(NO_PARENT).expect("NO_PARENT is a valid hash")
            });

            let values = (
                b::hash.eq(hash.hash_hex()),
                b::number.eq(number),
                b::parent_hash.eq(parent_hash.hash_hex()),
                b::network_name.eq(chain),
                b::data.eq(data),
            );

            insert_into(b::table)
                .values(values.clone())
                .on_conflict(b::hash)
                .do_update()
                .set(values)
                .execute(conn)?;
            Ok(())
        }

        pub(super) fn missing_parent(
            &self,
            conn: &PgConnection,
            chain: &str,
            first_block: i64,
            hash: H256,
            genesis: H256,
        ) -> Result<Option<H256>, Error> {
            let missing_parent_sql: &str = "
            with recursive chain(hash, parent_hash, last) as (
                -- base case: look at the head candidate block
                select b.hash, b.parent_hash, false
                  from ethereum_blocks b
                 where b.network_name = $1
                   and b.hash = $2
                   and b.hash != $3
                union all
                -- recursion step: add a block whose hash is the latest parent_hash
                -- on chain
                select chain.parent_hash,
                       b.parent_hash,
                       coalesce(b.parent_hash is null
                             or b.number <= $4
                             or b.hash = $3, true)
                  from chain left outer join ethereum_blocks b
                              on chain.parent_hash = b.hash
                             and b.network_name = $1
                 where not chain.last)
             select hash
               from chain
              where chain.parent_hash is null;
            ";
            let hash = format!("{:x}", hash);
            let genesis = format!("{:x}", genesis);
            let missing = sql_query(missing_parent_sql)
                .bind::<Text, _>(chain)
                .bind::<Text, _>(&hash)
                .bind::<Text, _>(&genesis)
                .bind::<BigInt, _>(first_block)
                .load::<BlockHashText>(conn)?;

            let missing = match missing.len() {
                0 => None,
                1 => Some(missing[0].hash.parse()?),
                _ => unreachable!("the query can only return no or one row"),
            };
            Ok(missing)
        }

        pub(super) fn chain_head_candidate(
            &self,
            conn: &PgConnection,
            chain: &str,
        ) -> Result<Option<BlockPtr>, Error> {
            use crate::models::block::ethereum_blocks as b;
            use crate::models::block::ethereum_networks as n;

            let head = n::table
                .filter(n::name.eq(chain))
                .select(n::head_block_number)
                .first::<Option<i64>>(conn)?
                .unwrap_or(-1);
            let opt = b::table
                .filter(b::network_name.eq(chain))
                .filter(b::number.gt(head))
                .order_by((b::number.desc(), b::hash))
                .select((b::hash, b::number))
                .first::<(String, i64)>(conn)
                .optional()?
                .map(|(hash, number)| BlockPtr::try_from((hash.as_str(), number)))
                .transpose();
            opt.map_err(Error::from)
        }
    }
}
pub struct ChainStore {
    pool: ConnectionPool,
    pub chain: String,
    pub(crate) storage: data::Storage,
    genesis_block_ptr: BlockPtr,
    status: ChainStatus,
}

impl ChainStore {
    pub(crate) fn new(
        chain: String,
        storage: data::Storage,
        net_identifier: &ChainIdentifier,
        status: ChainStatus,
        pool: ConnectionPool,
    ) -> Self {
        let store = ChainStore {
            pool,
            chain,
            storage,
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
        let pool = self.pool.clone();
        let network = self.chain.clone();
        let storage = self.storage.clone();
        pool.with_conn(move |conn, _| {
            conn.transaction(|| {
                storage
                    .upsert_block(&conn, &network, block.as_ref(), true)
                    .map_err(CancelableError::from)
            })
        })
        .await
        .map_err(Error::from)
    }

    fn upsert_light_blocks(&self, blocks: &[&dyn Block]) -> Result<(), Error> {
        unimplemented!();
    }

    async fn attempt_chain_head_update(
        self: Arc<Self>,
        _ancestor_count: BlockNumber,
    ) -> Result<Option<H256>, Error> {
        use crate::models::block::ethereum_networks as n;

        let chain_store = self.clone();
        let missing: Option<H256> = self
            .pool
            .with_conn(move |conn, _| {
                let candidate = chain_store
                    .storage
                    .chain_head_candidate(&conn, &chain_store.chain)
                    .map_err(CancelableError::from)?;

                let ptr = match &candidate {
                    None => return Ok(None),
                    Some(ptr) => ptr,
                };

                let hash = ptr.hash_hex();
                let number = ptr.number as i64;

                conn.transaction(|| -> Result<Option<H256>, StoreError> {
                    diesel::update(n::table.filter(n::name.eq(&chain_store.chain)))
                        .set((
                            n::head_block_hash.eq(&hash),
                            n::head_block_number.eq(number),
                        ))
                        .execute(conn)?;
                    Ok(None)
                })
                .map_err(CancelableError::from)
            })
            .await?;
        Ok(missing)
    }

    fn chain_head_ptr(&self) -> Result<Option<BlockPtr>, Error> {
        use crate::models::block::ethereum_blocks::dsl::*;
        let row = ethereum_blocks
            .select((hash, number))
            .filter(network_name.eq(self.chain.clone()))
            .first::<(String, i64)>(&self.get_conn()?)
            .optional()
            .map_err(Error::from)?;
        row.map(|(block_hash, block_number)| {
            BlockPtr::try_from((block_hash.as_str(), block_number))
        })
        .transpose()
    }

    fn blocks(&self, hashes: &[H256]) -> Result<Vec<json::Value>, Error> {
        unimplemented!();
    }

    fn ancestor_block(
        &self,
        _block_ptr: BlockPtr,
        _offset: BlockNumber,
    ) -> Result<Option<json::Value>, Error> {
        unimplemented!();
    }
}
