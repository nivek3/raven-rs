use async_trait::async_trait;
use raven_engine::{EntityChange, EntityValue, LiteBlockHeader, Network, RavenResult};
use raven_postgres::{PostgresChainStore, PostgresError, PostgresStorage};
use sqlx::{AssertSqlSafe, PgConnection, PgPool, postgres::PgPoolOptions, types::Json};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

// Versioned test entities support arbitrary JSON fixtures and BIGINT block positions.
pub const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS test_entity (
    entity_type TEXT NOT NULL, entity_id TEXT NOT NULL, data JSONB NOT NULL,
    start_block BIGINT NOT NULL, end_block BIGINT,
    PRIMARY KEY(entity_type, entity_id, start_block));
    CREATE UNIQUE INDEX IF NOT EXISTS test_entity_current ON test_entity(entity_type, entity_id) WHERE end_block IS NULL;";
pub struct FixtureStorage;
#[async_trait]
impl PostgresStorage for FixtureStorage {
    /// Reads the current version of a fixture entity.
    async fn get_entity(
        &self,
        conn: &mut PgConnection,
        kind: &str,
        id: &str,
    ) -> RavenResult<Option<EntityValue>> {
        Ok(sqlx::query_scalar::<_, Json<EntityValue>>("SELECT data FROM test_entity WHERE entity_type = $1 AND entity_id = $2 AND end_block IS NULL")
            .bind(kind).bind(id).fetch_optional(conn).await.map_err(PostgresError::Database)?.map(|row| row.0))
    }
    /// Applies the fixture entity changes at one block height.
    async fn apply_block(
        &self,
        conn: &mut PgConnection,
        number: u64,
        changes: &[EntityChange],
    ) -> RavenResult<()> {
        for change in changes {
            sqlx::query("DELETE FROM test_entity WHERE entity_type = $1 AND entity_id = $2 AND start_block = $3::TEXT::BIGINT")
                .bind(&change.entity_type).bind(&change.entity_id).bind(number.to_string()).execute(&mut *conn).await.map_err(PostgresError::Database)?;
            sqlx::query("UPDATE test_entity SET end_block = $3::TEXT::BIGINT WHERE entity_type = $1 AND entity_id = $2 AND end_block IS NULL")
                .bind(&change.entity_type).bind(&change.entity_id).bind(number.to_string()).execute(&mut *conn).await.map_err(PostgresError::Database)?;
            if let Some(data) = &change.data {
                sqlx::query("INSERT INTO test_entity(entity_type, entity_id, data, start_block) VALUES($1, $2, $3, $4::TEXT::BIGINT)")
                    .bind(&change.entity_type).bind(&change.entity_id).bind(Json(data)).bind(number.to_string()).execute(&mut *conn).await.map_err(PostgresError::Database)?;
            }
        }
        Ok(())
    }
    /// Removes fixture versions created by the reverted block or later.
    async fn revert_block(&self, conn: &mut PgConnection, number: u64) -> RavenResult<()> {
        sqlx::query("DELETE FROM test_entity WHERE start_block >= $1::TEXT::BIGINT")
            .bind(number.to_string())
            .execute(&mut *conn)
            .await
            .map_err(PostgresError::Database)?;
        sqlx::query("UPDATE test_entity SET end_block = NULL WHERE end_block >= $1::TEXT::BIGINT")
            .bind(number.to_string())
            .execute(conn)
            .await
            .map_err(PostgresError::Database)?;
        Ok(())
    }
}

pub type TestStore = PostgresChainStore<String, u64>;
static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Database {
    pub(super) url: String,
    pub schema: String,
    pub pool: PgPool,
}

impl Database {
    /// Creates a disposable database schema and connection pool for one test.
    pub async fn new() -> Self {
        let url = std::env::var("RAVEN_TEST_DATABASE_URL").expect(
            "set RAVEN_TEST_DATABASE_URL to a dedicated disposable PostgreSQL test database",
        );
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .unwrap_or_else(|_| panic!("cannot connect to the dedicated PostgreSQL test database"));
        let schema = format!(
            "raven_test_{}_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        Self { url, schema, pool }
    }
    /// Counts rows in an allowed fixture table.
    pub async fn count(&self, table: &str) -> i64 {
        assert!(["test_entity", "blocks", "networks", "counter"].contains(&table));
        sqlx::query_scalar(AssertSqlSafe(format!(
            r#"SELECT count(*) FROM "{}".{table}"#,
            self.schema
        )))
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
    /// Drops the disposable schema and closes its pool.
    pub async fn cleanup(self) {
        assert!(self.schema.starts_with("raven_test_"));
        sqlx::query(AssertSqlSafe(format!(
            r#"DROP SCHEMA "{}" CASCADE"#,
            self.schema
        )))
        .execute(&self.pool)
        .await
        .unwrap();
        self.pool.close().await;
    }
}

/// Builds a fixture header from explicit chain coordinates.
pub fn block(number: u64, hash: &str, parent: &str) -> LiteBlockHeader<String> {
    LiteBlockHeader {
        number,
        hash: hash.into(),
        parent_hash: parent.into(),
    }
}
/// Returns the standard fixture network beginning at block ten.
pub fn network() -> Network<String, u64> {
    Network::new(1, 10, Some(block(9, "b9", "b8"))).unwrap()
}
