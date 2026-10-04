//! Application-owned entity storage within Raven's PostgreSQL transactions.
use async_trait::async_trait;
use raven_engine::{EntityChange, EntityValue, RavenResult};
use sqlx::PgConnection;

/// Applications own their tables, mapping and version rules. The connection has
/// the index schema as search_path and belongs to Raven's transaction. Methods
/// must use this connection for all state changes and must not commit, roll back,
/// change session settings, or publish external side effects.
#[async_trait]
pub trait PostgresStorage: Send + Sync {
    /// Read the current committed application entity. Do not write in this method.
    async fn get_entity(
        &self,
        conn: &mut PgConnection,
        entity_type: &str,
        id: &str,
    ) -> RavenResult<Option<EntityValue>>;

    /// Apply the final changes from one block, preserving any history required
    /// for rollback. An empty slice still represents an indexed empty block.
    /// Raven publishes the block pointer only after this succeeds.
    async fn apply_block(
        &self,
        conn: &mut PgConnection,
        block_number: u64,
        changes: &[EntityChange],
    ) -> RavenResult<()>;

    /// Undo exactly the current head block, including entity existence and prior
    /// values. Raven updates canonical metadata and progress in the same transaction.
    /// This must work after a process restart using application-owned persisted data.
    ///
    /// `block_number` is the canonical head being removed.
    async fn revert_block(&self, conn: &mut PgConnection, block_number: u64) -> RavenResult<()>;
}
