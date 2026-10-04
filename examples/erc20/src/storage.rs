//! Application-owned ERC20 SQL and serde mapping.
use async_trait::async_trait;
use raven_engine::{EntityChange, EntityValue, RavenResult};
use raven_postgres::{PostgresError, PostgresStorage};
use sqlx::{PgConnection, types::Json};

use crate::{ExampleError, entities::ERC20Balance};

pub(crate) const SCHEMA_SQL: &str = include_str!("schema.sql");

pub(crate) struct Storage;

#[async_trait]
impl PostgresStorage for Storage {
    /// Reads one current entity by type and ID from the ERC-20 projection.
    async fn get_entity(
        &self,
        conn: &mut PgConnection,
        kind: &str,
        id: &str,
    ) -> RavenResult<Option<EntityValue>> {
        let row = sqlx::query_scalar::<_, Json<EntityValue>>(
            "SELECT data FROM erc20_state WHERE entity_type = $1 AND entity_id = $2",
        )
        .bind(kind)
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(PostgresError::Database)?;
        Ok(row.map(|row| row.0))
    }

    /// Applies all entity changes for a block through the schema procedure.
    async fn apply_block(
        &self,
        conn: &mut PgConnection,
        number: u64,
        changes: &[EntityChange],
    ) -> RavenResult<()> {
        let height =
            i64::try_from(number).map_err(|_| ExampleError::InvalidEntityState("block number"))?;
        let changes: Vec<_> = changes
            .iter()
            .map(|change| {
                serde_json::json!({
                    "kind": change.entity_type, "id": change.entity_id, "data": change.data,
                })
            })
            .collect();
        sqlx::query("SELECT erc20_apply($1, $2)")
            .bind(height)
            .bind(Json(changes))
            .execute(conn)
            .await
            .map_err(PostgresError::Database)?;
        Ok(())
    }

    /// Reverts the projection changes recorded for one block.
    async fn revert_block(&self, conn: &mut PgConnection, number: u64) -> RavenResult<()> {
        let height =
            i64::try_from(number).map_err(|_| ExampleError::InvalidEntityState("block number"))?;
        sqlx::query("SELECT erc20_revert($1)")
            .bind(height)
            .execute(conn)
            .await
            .map_err(PostgresError::Database)?;
        Ok(())
    }
}

/// Read balances at one canonical height; None selects the committed head.
/// The caller supplies a read snapshot with this index's schema as search_path.
pub async fn balances_at(
    conn: &mut PgConnection,
    number: Option<u64>,
    account: Option<&str>,
) -> RavenResult<Vec<ERC20Balance>> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "WITH checkpoint AS (
            SELECT COALESCE($1::text::numeric, latest_block_number)::bigint AS height
            FROM networks WHERE singleton AND latest_block_number IS NOT NULL
              AND COALESCE($1::text::numeric, latest_block_number) BETWEEN start_block AND latest_block_number
         ) SELECT jsonb_build_object('id', id, 'contract', contract, 'account', account,
                    'value', value::text, 'valueExact', value_exact::text)
         FROM erc20_balance CROSS JOIN checkpoint
         WHERE block_range @> checkpoint.height AND ($2::text IS NULL OR account = $2)
         ORDER BY id",
    ).bind(number.map(|number| number.to_string())).bind(account)
        .fetch_all(conn).await.map_err(PostgresError::Database)?;
    rows.into_iter()
        .map(|row| {
            serde_json::from_value(row.0)
                .map_err(|_| ExampleError::InvalidEntityState("ERC20Balance").into())
        })
        .collect()
}
