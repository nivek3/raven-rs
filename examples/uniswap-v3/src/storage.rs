//! Application-owned native pool event storage.
use crate::ExampleError;
use async_trait::async_trait;
use raven_engine::{EntityChange, EntityValue, RavenResult};
use raven_postgres::{PostgresError, PostgresStorage};
use sqlx::{PgConnection, types::Json};
pub(crate) const SCHEMA_SQL: &str = include_str!("schema.sql");

pub(crate) struct Storage;

#[async_trait]
impl PostgresStorage for Storage {
    /// Reads one current entity projection from the native state view.
    async fn get_entity(
        &self,
        conn: &mut PgConnection,
        kind: &str,
        id: &str,
    ) -> RavenResult<Option<EntityValue>> {
        let query = match kind {
            "Mint" => {
                "SELECT data FROM pool_state WHERE entity_type = $1 AND entity_id = $2 \
                 AND block_number = (SELECT block_number FROM mint WHERE id = $2::NUMERIC)"
            }
            "Burn" => {
                "SELECT data FROM pool_state WHERE entity_type = $1 AND entity_id = $2 \
                 AND block_number = (SELECT block_number FROM burn WHERE id = $2::NUMERIC)"
            }
            "Swap" => {
                "SELECT data FROM pool_state WHERE entity_type = $1 AND entity_id = $2 \
                 AND block_number = (SELECT block_number FROM swap WHERE id = $2::NUMERIC)"
            }
            _ => "SELECT data FROM pool_state WHERE entity_type = $1 AND entity_id = $2",
        };
        let row = sqlx::query_scalar::<_, Json<EntityValue>>(query)
            .bind(kind)
            .bind(id)
            .fetch_optional(conn)
            .await
            .map_err(PostgresError::Database)?;
        Ok(row.map(|row| row.0))
    }

    /// Applies one block's staged changes through the application SQL function.
    async fn apply_block(
        &self,
        conn: &mut PgConnection,
        number: u64,
        changes: &[EntityChange],
    ) -> RavenResult<()> {
        let height = i64::try_from(number).map_err(|_| ExampleError::InvalidPoolState)?;
        let changes: Vec<_> = changes
            .iter()
            .map(|change| {
                serde_json::json!({
                    "kind": change.entity_type, "id": change.entity_id, "data": change.data,
                })
            })
            .collect();
        sqlx::query("SELECT pool_apply($1, $2)")
            .bind(height)
            .bind(Json(changes))
            .execute(conn)
            .await
            .map_err(PostgresError::Database)?;
        Ok(())
    }

    /// Reverts native entity versions created at the supplied block height.
    async fn revert_block(&self, conn: &mut PgConnection, number: u64) -> RavenResult<()> {
        let height = i64::try_from(number).map_err(|_| ExampleError::InvalidPoolState)?;
        sqlx::query("SELECT pool_revert($1)")
            .bind(height)
            .execute(conn)
            .await
            .map_err(PostgresError::Database)?;
        Ok(())
    }
}
