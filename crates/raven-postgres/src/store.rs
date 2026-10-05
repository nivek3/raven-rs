//! PostgreSQL implementation of Raven's single-writer store.
//!
//! The store owns one advisory-locked connection pool, creates one dedicated
//! schema and publishes each block's application state and block pointer atomically.

use std::marker::PhantomData;

use async_trait::async_trait;
use raven_engine::{
    BlockPtr, ChainStore, EngineError, EntityChange, EntityValue, LiteBlockHeader, Network,
    RavenResult,
};
use serde::{Serialize, de::DeserializeOwned};
use sqlx::{
    AssertSqlSafe, PgConnection, PgPool, Postgres, Row, Transaction,
    postgres::{PgPoolOptions, PgRow},
    types::Json,
};

use crate::{PostgresError, PostgresStorage, error::database};

/// Persisted block status; values also define the partial index in schema.sql.
#[repr(i16)]
enum BlockStatus {
    Canonical = 1,
    Orphaned = 0,
}

/// A single writer for one dedicated schema. Hashes round-trip through serde JSON;
/// chain identities round-trip through u64. Network names come from configuration.
/// Application tables may coexist in this schema; applications must not modify
/// the core store tables. Do not use a transaction-pooling PostgreSQL proxy.
/// A replacement connection never reacquires ownership: reopen the ChainStore instead.
pub struct PostgresChainStore<H, I> {
    pool: PgPool,
    schema: String,
    network_name: String,
    pid: i32,
    storage: Box<dyn PostgresStorage>,
    marker: PhantomData<(H, I)>,
}

impl<H, I> PostgresChainStore<H, I> {
    /// Acquires writer ownership before creating the schema and tables. Requires
    /// CREATE privilege for a new schema, or ownership of an existing Raven schema.
    pub async fn connect(
        url: &str,
        schema: &str,
        network_name: &str,
        storage: impl PostgresStorage + 'static,
    ) -> RavenResult<Self> {
        Self::connect_with_schema_sql(url, schema, network_name, "", storage).await
    }

    /// Installs application-owned tables and functions in the same transaction
    /// as the Raven schema, after acquiring writer ownership. SQL runs with the
    /// dedicated schema as search_path and must come from trusted application
    /// source, never from user configuration. An error rolls back initialization.
    pub async fn connect_with_schema_sql(
        url: &str,
        schema: &str,
        network_name: &str,
        schema_sql: &str,
        storage: impl PostgresStorage + 'static,
    ) -> RavenResult<Self> {
        validate_schema(schema)?;
        if network_name.trim().is_empty() {
            return Err(PostgresError::InvalidNetworkName.into());
        }
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .test_before_acquire(false)
            .connect(url)
            .await
            .map_err(database)?;
        let mut tx = pool.begin().await.map_err(database)?;
        let owned: bool = sqlx::query_scalar(
            "SELECT pg_try_advisory_lock(hashtext(current_database()), hashtext($1))",
        )
        .bind(schema)
        .fetch_one(&mut *tx)
        .await
        .map_err(database)?;
        if !owned {
            return Err(PostgresError::WriterBusy.into());
        }
        let writer_pid = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
        // Schema is validated as a plain identifier; quote it for reserved words.
        sqlx::query(AssertSqlSafe(format!(
            r#"CREATE SCHEMA IF NOT EXISTS "{schema}""#
        )))
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        set_schema(&mut tx, schema).await?;
        sqlx::Executor::execute(&mut *tx, include_str!("schema.sql"))
            .await
            .map_err(database)?;
        let persisted_name: Option<String> =
            sqlx::query_scalar("SELECT network_name FROM networks WHERE singleton")
                .fetch_optional(&mut *tx)
                .await
                .map_err(database)?;
        if persisted_name
            .as_deref()
            .is_some_and(|name| name != network_name)
        {
            return Err(PostgresError::NetworkConflict.into());
        }
        if !schema_sql.is_empty() {
            sqlx::Executor::execute(&mut *tx, AssertSqlSafe(schema_sql))
                .await
                .map_err(database)?;
        }
        tx.commit().await.map_err(database)?;
        Ok(Self {
            pool,
            schema: schema.to_owned(),
            network_name: network_name.to_owned(),
            pid: writer_pid,
            storage: Box::new(storage),
            marker: PhantomData,
        })
    }

    /// Waits for outstanding transactions and releases the writer connection.
    pub async fn close(self) {
        self.pool.close().await;
    }

    /// Opens a transaction that still owns the schema's writer lock.
    async fn transaction(&self) -> RavenResult<Transaction<'static, Postgres>> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        verify_writer(&mut tx, &self.schema, self.pid).await?;
        set_schema(&mut tx, &self.schema).await?;
        Ok(tx)
    }
}

/// Rejects schema names that cannot safely be used as dedicated identifiers.
fn validate_schema(schema: &str) -> RavenResult<()> {
    if schema.is_empty()
        || schema.len() > 63
        || !schema.as_bytes()[0].is_ascii_lowercase()
        || !schema
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        || schema == "public"
        || schema.starts_with("pg_")
    {
        return Err(PostgresError::InvalidSchema.into());
    }
    Ok(())
}

/// Restricts the current transaction's search path to the indexing schema.
async fn set_schema(conn: &mut PgConnection, schema: &str) -> RavenResult<()> {
    // LOCAL prevents the schema from leaking beyond the current transaction.
    sqlx::query("SELECT set_config('search_path', $1, true)")
        .bind(format!(r#""{schema}", pg_catalog"#))
        .execute(conn)
        .await
        .map_err(database)?;
    Ok(())
}

/// Confirms that this transaction still runs on the advisory-lock owner connection.
async fn verify_writer(conn: &mut PgConnection, schema: &str, pid: i32) -> RavenResult<()> {
    let owned: bool = sqlx::query_scalar(
        "SELECT pg_backend_pid() = $2 AND EXISTS (
            SELECT 1 FROM pg_catalog.pg_locks
            WHERE locktype = 'advisory' AND pid = pg_backend_pid() AND granted
              AND database = (SELECT oid FROM pg_catalog.pg_database WHERE datname = current_database())
              AND classid::bigint = (hashtext(current_database())::bigint & 4294967295)
              AND objid::bigint = (hashtext($1)::bigint & 4294967295) AND objsubid = 2
        )"
    ).bind(schema).bind(pid).fetch_one(conn).await.map_err(database)?;
    if !owned {
        return Err(PostgresError::WriterLost.into());
    }
    Ok(())
}

/// Reads a persisted BIGINT height encoded as text.
fn height(row: &PgRow, name: &str) -> RavenResult<u64> {
    row.try_get::<String, _>(name)
        .map_err(database)?
        .parse()
        .map_err(|_| PostgresError::InvalidState.into())
}

/// Deserializes a stored block row into its lightweight canonical header.
fn header<H: DeserializeOwned>(row: &PgRow) -> RavenResult<LiteBlockHeader<H>> {
    Ok(LiteBlockHeader {
        number: height(row, "number")?,
        hash: row.try_get::<Json<H>, _>("hash").map_err(database)?.0,
        parent_hash: row
            .try_get::<Json<H>, _>("parent_hash")
            .map_err(database)?
            .0,
    })
}

/// Reads the latest canonical block pointer, if indexing has started.
async fn read_block_ptr<H: DeserializeOwned>(
    conn: &mut PgConnection,
) -> RavenResult<Option<BlockPtr<H>>> {
    let row = sqlx::query("SELECT latest_block_number::text AS number, latest_block_hash AS hash FROM networks WHERE singleton AND latest_block_number IS NOT NULL")
        .fetch_optional(conn).await.map_err(database)?;
    row.map(|row| {
        Ok(BlockPtr {
            number: height(&row, "number")?,
            hash: row.try_get::<Json<H>, _>("hash").map_err(database)?.0,
        })
    })
    .transpose()
}

/// Reads the canonical header at one block height.
async fn read_header<H: DeserializeOwned>(
    conn: &mut PgConnection,
    number: u64,
) -> RavenResult<Option<LiteBlockHeader<H>>> {
    sqlx::query("SELECT number::text, hash, parent_hash FROM blocks WHERE number = $1::text::bigint AND status = 1")
        .bind(number.to_string()).fetch_optional(conn).await.map_err(database)?
        .as_ref().map(header).transpose()
}

/// Reconstructs persisted network metadata for the configured schema.
async fn read_network<H: DeserializeOwned, I: From<u64>>(
    conn: &mut PgConnection,
) -> RavenResult<Option<Network<H, I>>> {
    let row = sqlx::query("SELECT chain_id::text, start_block::text, start_parent_number::text, start_parent_hash, start_parent_parent_hash FROM networks WHERE singleton")
        .fetch_optional(conn).await.map_err(database)?;
    row.map(|row| {
        let start = height(&row, "start_block")?;
        let parent = if start == 0 {
            None
        } else {
            Some(LiteBlockHeader {
                number: height(&row, "start_parent_number")?,
                hash: row
                    .try_get::<Json<H>, _>("start_parent_hash")
                    .map_err(database)?
                    .0,
                parent_hash: row
                    .try_get::<Json<H>, _>("start_parent_parent_hash")
                    .map_err(database)?
                    .0,
            })
        };
        Ok(Network::new(
            I::from(height(&row, "chain_id")?),
            start,
            parent,
        )?)
    })
    .transpose()
}

#[async_trait]
impl<H, I> ChainStore for PostgresChainStore<H, I>
where
    H: Clone + Eq + Serialize + DeserializeOwned + Send + Sync + 'static,
    I: Clone + Eq + From<u64> + Into<u64> + Send + Sync + 'static,
{
    type Hash = H;
    type ChainIdentity = I;

    /// Returns the initialized network metadata, if present.
    async fn network(&self) -> RavenResult<Option<Network<H, I>>> {
        let mut tx = self.transaction().await?;
        let result = read_network(&mut tx).await?;
        tx.commit().await.map_err(database)?;
        Ok(result)
    }

    /// Persists the network once and rejects a conflicting initialization.
    async fn initialize(&self, network: &Network<H, I>) -> RavenResult<()> {
        let mut tx = self.transaction().await?;
        if let Some(existing) = read_network::<H, I>(&mut tx).await? {
            if &existing != network {
                return Err(PostgresError::NetworkConflict.into());
            }
        } else {
            let dirty: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blocks)")
                .fetch_one(&mut *tx)
                .await
                .map_err(database)?;
            if dirty {
                return Err(PostgresError::InvalidState.into());
            }
            sqlx::query("INSERT INTO networks (chain_id, network_name, start_block, start_parent_number, start_parent_hash, start_parent_parent_hash) VALUES ($1::text::numeric, $2, $3::text::bigint, $4::text::bigint, $5, $6)")
                .bind(Into::<u64>::into(network.chain_identity().clone()).to_string())
                .bind(&self.network_name).bind(network.start_block().to_string())
                .bind(network.parent().map(|h| h.number.to_string()))
                .bind(network.parent().map(|h| Json(&h.hash)))
                .bind(network.parent().map(|h| Json(&h.parent_hash)))
                .execute(&mut *tx).await.map_err(database)?;
        }
        tx.commit().await.map_err(database)?;
        Ok(())
    }

    /// Returns the latest canonical block pointer.
    async fn block_ptr(&self) -> RavenResult<Option<BlockPtr<H>>> {
        let mut tx = self.transaction().await?;
        let result = read_block_ptr(&mut tx).await?;
        tx.commit().await.map_err(database)?;
        Ok(result)
    }

    /// Returns the canonical header stored at the requested height.
    async fn block_header_by_number(&self, number: u64) -> RavenResult<Option<LiteBlockHeader<H>>> {
        let mut tx = self.transaction().await?;
        let result = read_header(&mut tx, number).await?;
        tx.commit().await.map_err(database)?;
        Ok(result)
    }

    /// Reads an entity after checking the caller's canonical block pointer.
    async fn get_entity(
        &self,
        expected: Option<&BlockPtr<H>>,
        entity_type: &str,
        id: &str,
    ) -> RavenResult<Option<EntityValue>> {
        let mut tx = self.transaction().await?;
        if read_block_ptr::<H>(&mut tx).await?.as_ref() != expected {
            return Err(EngineError::BlockPtrConflict.into());
        }
        let data = self.storage.get_entity(&mut tx, entity_type, id).await?;
        tx.commit().await.map_err(database)?;
        Ok(data)
    }

    /// Atomically persists one next canonical block and its application changes.
    async fn commit_block(
        &self,
        expected: Option<&BlockPtr<H>>,
        next: &LiteBlockHeader<H>,
        changes: Vec<EntityChange>,
    ) -> RavenResult<()> {
        let mut tx = self.transaction().await?;
        let block_ptr = read_block_ptr::<H>(&mut tx).await?;
        if block_ptr.as_ref() != expected {
            return Err(EngineError::BlockPtrConflict.into());
        }
        let network = read_network::<H, I>(&mut tx)
            .await?
            .ok_or(PostgresError::InvalidState)?;
        match block_ptr {
            Some(cp) => {
                if cp.number < network.start_block() {
                    return Err(PostgresError::InvalidState.into());
                }
                let previous = read_header::<H>(&mut tx, cp.number)
                    .await?
                    .ok_or(PostgresError::InvalidState)?;
                if previous.hash != cp.hash || !next.extends(&previous) {
                    return Err(EngineError::InvalidBlock.into());
                }
            }
            None if !network.accepts_first(next) => return Err(EngineError::InvalidBlock.into()),
            None => {}
        }
        let existing = sqlx::query(
            "SELECT number::text, hash, parent_hash, status FROM blocks WHERE hash = $1",
        )
        .bind(Json(&next.hash))
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        if let Some(row) = existing
            && (header::<H>(&row)? != *next
                || row.try_get::<i16, _>("status").map_err(database)?
                    != BlockStatus::Orphaned as i16)
        {
            return Err(PostgresError::InvalidState.into());
        }
        sqlx::query("INSERT INTO blocks (number, hash, parent_hash, status) VALUES ($1::text::bigint, $2, $3, $4) ON CONFLICT (hash) DO UPDATE SET status = EXCLUDED.status")
            .bind(next.number.to_string()).bind(Json(&next.hash)).bind(Json(&next.parent_hash))
            .bind(BlockStatus::Canonical as i16)
            .execute(&mut *tx).await.map_err(database)?;
        self.storage
            .apply_block(&mut tx, next.number, &changes)
            .await?;
        verify_writer(&mut tx, &self.schema, self.pid).await?;
        let result = sqlx::query("UPDATE networks SET latest_block_number = $1::text::bigint, latest_block_hash = $2 WHERE singleton")
            .bind(next.number.to_string()).bind(Json(&next.hash))
            .execute(&mut *tx).await.map_err(database)?;
        if result.rows_affected() != 1 {
            return Err(PostgresError::InvalidState.into());
        }
        tx.commit().await.map_err(database)?;
        Ok(())
    }

    /// Atomically removes the current canonical head and restores its parent pointer.
    async fn revert_block(&self, target: &LiteBlockHeader<H>) -> RavenResult<()> {
        let mut tx = self.transaction().await?;
        if read_block_ptr::<H>(&mut tx).await? != Some(BlockPtr::from(target)) {
            return Err(EngineError::BlockPtrConflict.into());
        }
        if read_header::<H>(&mut tx, target.number).await?.as_ref() != Some(target) {
            return Err(PostgresError::InvalidState.into());
        }
        let network = read_network::<H, I>(&mut tx)
            .await?
            .ok_or(PostgresError::InvalidState)?;
        let parent = if target.number == network.start_block() && network.accepts_first(target) {
            None
        } else if target.number > network.start_block() {
            let parent = read_header::<H>(&mut tx, target.number - 1)
                .await?
                .ok_or(PostgresError::InvalidState)?;
            if !target.extends(&parent) {
                return Err(PostgresError::InvalidState.into());
            }
            Some(parent)
        } else {
            return Err(PostgresError::InvalidState.into());
        };
        self.storage.revert_block(&mut tx, target.number).await?;
        verify_writer(&mut tx, &self.schema, self.pid).await?;
        sqlx::query("UPDATE blocks SET status = $2 WHERE hash = $1")
            .bind(Json(&target.hash))
            .bind(BlockStatus::Orphaned as i16)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        match parent {
            Some(parent) => {
                sqlx::query("UPDATE networks SET latest_block_number = $1::text::bigint, latest_block_hash = $2 WHERE singleton")
                .bind(parent.number.to_string()).bind(Json(&parent.hash)).execute(&mut *tx).await.map_err(database)?;
            }
            None => {
                sqlx::query("UPDATE networks SET latest_block_number = NULL, latest_block_hash = NULL WHERE singleton")
                    .execute(&mut *tx)
                    .await
                    .map_err(database)?;
            }
        }
        tx.commit().await.map_err(database)?;
        Ok(())
    }
}
