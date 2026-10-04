mod fake;
use fake::{Database, FixtureStorage, SCHEMA_SQL, TestStore, block, network};
use raven_engine::{BlockPtr, ChainStore, EntityChange};
use raven_postgres::PostgresStorage;
use serde_json::json;
use sqlx::{PgConnection, PgPool, Postgres, Transaction, types::Json};

impl Database {
    /// Opens a fixture store after appending caller-provided initialization SQL.
    async fn try_open_with_sql(&self, sql: &str) -> raven_engine::RavenResult<TestStore> {
        TestStore::connect_with_schema_sql(
            &self.url,
            &self.schema,
            "test-chain",
            &format!("{SCHEMA_SQL}\n{sql}"),
            FixtureStorage,
        )
        .await
    }

    /// Opens a fixture store with caller-provided storage behavior.
    async fn open_storage(&self, sql: &str, storage: impl PostgresStorage + 'static) -> TestStore {
        TestStore::connect_with_schema_sql(&self.url, &self.schema, "test-chain", sql, storage)
            .await
            .unwrap()
    }
}

// All entity mapping and versions belong to this test application.
const APPLICATION_SQL: &str = "
    CREATE TABLE IF NOT EXISTS counter (id TEXT NOT NULL, block_range INT8RANGE NOT NULL, value NUMERIC NOT NULL);
    CREATE UNIQUE INDEX IF NOT EXISTS counter_current ON counter(id) WHERE upper_inf(block_range);";
struct CounterStorage {
    fail_revert: bool,
}

#[async_trait::async_trait]
impl PostgresStorage for CounterStorage {
    /// Reads the current version of a counter entity.
    async fn get_entity(
        &self,
        conn: &mut PgConnection,
        kind: &str,
        id: &str,
    ) -> raven_engine::RavenResult<Option<raven_engine::EntityValue>> {
        if kind != "Counter" {
            return Ok(None);
        }
        Ok(sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT jsonb_build_object('value', value::TEXT) FROM counter WHERE id = $1 AND upper_inf(block_range)",
        ).bind(id).fetch_optional(conn).await.map_err(raven_postgres::PostgresError::Database)?.map(|row| row.0))
    }
    /// Applies counter versions for all changes in one block.
    async fn apply_block(
        &self,
        conn: &mut PgConnection,
        number: u64,
        changes: &[EntityChange],
    ) -> raven_engine::RavenResult<()> {
        let height = i64::try_from(number).unwrap();
        for change in changes {
            sqlx::query("UPDATE counter SET block_range = int8range(lower(block_range), $2, '[)') WHERE id = $1 AND upper_inf(block_range)")
                .bind(&change.entity_id).bind(height).execute(&mut *conn).await.map_err(raven_postgres::PostgresError::Database)?;
            if let Some(data) = &change.data {
                sqlx::query("INSERT INTO counter(id, block_range, value) VALUES($1, int8range($2, NULL, '[)'), ($3->>'value')::NUMERIC)")
                    .bind(&change.entity_id).bind(height).bind(Json(data)).execute(&mut *conn).await.map_err(raven_postgres::PostgresError::Database)?;
            }
        }
        Ok(())
    }
    /// Removes versions created at or after the reverted block.
    async fn revert_block(
        &self,
        conn: &mut PgConnection,
        number: u64,
    ) -> raven_engine::RavenResult<()> {
        let height = i64::try_from(number).unwrap();
        sqlx::query("DELETE FROM counter WHERE lower(block_range) >= $1")
            .bind(height)
            .execute(&mut *conn)
            .await
            .map_err(raven_postgres::PostgresError::Database)?;
        if self.fail_revert {
            sqlx::query("SELECT 1 / 0")
                .execute(&mut *conn)
                .await
                .map_err(raven_postgres::PostgresError::Database)?;
        }
        sqlx::query("UPDATE counter SET block_range = int8range(lower(block_range), NULL, '[)') WHERE upper(block_range) >= $1")
            .bind(height).execute(conn).await.map_err(raven_postgres::PostgresError::Database)?;
        Ok(())
    }
}

/// Opens a read-only, repeatable-read transaction scoped to the application schema.
async fn read_snapshot(pool: &PgPool, schema: &str) -> Transaction<'static, Postgres> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('search_path', $1, true)")
        .bind(format!("\"{schema}\", pg_catalog"))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

/// Creates a change for the fixture counter entity.
fn change(value: &str) -> EntityChange {
    EntityChange {
        entity_type: "Counter".into(),
        entity_id: "main".into(),
        data: Some(json!({"value": value})),
    }
}

/// Reads the current fixture counter value.
async fn value(conn: &mut PgConnection) -> String {
    sqlx::query_scalar(
        "SELECT value::text FROM counter WHERE id = 'main' AND upper_inf(block_range)",
    )
    .fetch_one(conn)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL; creates and drops a unique schema"]
/// Verifies that application sql commits and rolls back with entities and progress.
async fn application_sql_commits_and_rolls_back_with_entities_and_progress() {
    let db = Database::new().await;
    let store = db
        .open_storage(APPLICATION_SQL, CounterStorage { fail_revert: false })
        .await;
    store.initialize(&network()).await.unwrap();
    let b10 = block(10, "b10", "b9");
    let b11 = block(11, "b11", "b10");
    store
        .commit_block(None, &b10, vec![change("1000")])
        .await
        .unwrap();
    let before = store
        .get_entity(Some(&BlockPtr::from(&b10)), "Counter", "main")
        .await
        .unwrap();
    assert!(
        store
            .commit_block(
                Some(&BlockPtr::from(&b10)),
                &b11,
                vec![change("not-a-number")]
            )
            .await
            .is_err()
    );
    assert_eq!(
        store
            .get_entity(Some(&BlockPtr::from(&b10)), "Counter", "main")
            .await
            .unwrap(),
        before
    );
    assert_eq!(db.count("counter").await, 1);
    assert_eq!(store.block_ptr().await.unwrap(), Some(BlockPtr::from(&b10)));
    let mut snapshot = read_snapshot(&db.pool, &db.schema).await;
    assert_eq!(value(&mut snapshot).await, "1000");
    snapshot.commit().await.unwrap();
    store
        .commit_block(Some(&BlockPtr::from(&b10)), &b11, vec![change("975")])
        .await
        .unwrap();
    store.close().await;
    let store = db
        .open_storage(APPLICATION_SQL, CounterStorage { fail_revert: false })
        .await;
    store.revert_block(&b11).await.unwrap();
    let mut snapshot = read_snapshot(&db.pool, &db.schema).await;
    assert_eq!(value(&mut snapshot).await, "1000");
    snapshot.commit().await.unwrap();
    assert_eq!(
        store
            .get_entity(Some(&BlockPtr::from(&b10)), "Counter", "main")
            .await
            .unwrap(),
        before
    );
    assert_eq!(db.count("counter").await, 1);
    store.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL; creates and drops a unique schema"]
/// Verifies that read only snapshots keep one state across commits and reorgs.
async fn read_only_snapshots_keep_one_state_across_commits_and_reorgs() {
    let db = Database::new().await;
    let store = db
        .open_storage(APPLICATION_SQL, CounterStorage { fail_revert: false })
        .await;
    store.initialize(&network()).await.unwrap();
    let b10 = block(10, "b10", "b9");
    let b11 = block(11, "b11", "b10");
    store
        .commit_block(None, &b10, vec![change("1000")])
        .await
        .unwrap();
    let mut original = read_snapshot(&db.pool, &db.schema).await;
    assert_eq!(value(&mut original).await, "1000");
    store
        .commit_block(Some(&BlockPtr::from(&b10)), &b11, vec![change("975")])
        .await
        .unwrap();
    let mut branch = read_snapshot(&db.pool, &db.schema).await;
    assert_eq!(value(&mut branch).await, "975");
    let hash: Json<String> =
        sqlx::query_scalar("SELECT latest_block_hash FROM networks WHERE singleton")
            .fetch_one(&mut *branch)
            .await
            .unwrap();
    assert_eq!(hash.0, "b11");
    store.revert_block(&b11).await.unwrap();
    assert_eq!(value(&mut original).await, "1000");
    assert_eq!(value(&mut branch).await, "975");
    original.commit().await.unwrap();
    branch.commit().await.unwrap();
    let mut current = read_snapshot(&db.pool, &db.schema).await;
    assert_eq!(value(&mut current).await, "1000");
    assert!(
        sqlx::query("UPDATE counter SET value = 0")
            .execute(&mut *current)
            .await
            .is_err()
    );
    current.rollback().await.unwrap();
    store.close().await;
    let mut tx = read_snapshot(&db.pool, &db.schema).await;
    let hash: Json<String> =
        sqlx::query_scalar("SELECT latest_block_hash FROM networks WHERE singleton")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(hash.0, "b10");
    tx.commit().await.unwrap();
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL; creates and drops a unique schema"]
/// Verifies that failed application initialization and reads do not leave partial schemas.
async fn failed_application_initialization_and_reads_do_not_leave_partial_schemas() {
    let db = Database::new().await;
    let mut tx = read_snapshot(&db.pool, &db.schema).await;
    assert!(
        sqlx::query("SELECT * FROM networks")
            .fetch_all(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(
        db.try_open_with_sql("CREATE TABLE partial (id integer); SELECT 1 / 0;")
            .await
            .is_err()
    );
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
            .bind(&db.schema)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(!exists);
    let store = db
        .open_storage(APPLICATION_SQL, CounterStorage { fail_revert: false })
        .await;
    let mut tx = read_snapshot(&db.pool, &db.schema).await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM networks")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(count, 0);
    tx.commit().await.unwrap();
    store.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL; creates and drops a unique schema"]
/// Verifies that failed application undo preserves versions and canonical progress.
async fn failed_application_undo_preserves_versions_and_canonical_progress() {
    let db = Database::new().await;
    let store = db
        .open_storage(APPLICATION_SQL, CounterStorage { fail_revert: false })
        .await;
    store.initialize(&network()).await.unwrap();
    let b10 = block(10, "b10", "b9");
    let b11 = block(11, "b11", "b10");
    store
        .commit_block(None, &b10, vec![change("1000")])
        .await
        .unwrap();
    store
        .commit_block(Some(&BlockPtr::from(&b10)), &b11, vec![change("975")])
        .await
        .unwrap();
    store.close().await;
    let store = db
        .open_storage(APPLICATION_SQL, CounterStorage { fail_revert: true })
        .await;
    assert!(store.revert_block(&b11).await.is_err());
    assert_eq!(store.block_ptr().await.unwrap(), Some(BlockPtr::from(&b11)));
    assert_eq!(
        store.block_header_by_number(11).await.unwrap(),
        Some(b11.clone())
    );
    assert_eq!(db.count("counter").await, 2);
    assert_eq!(
        store
            .get_entity(Some(&BlockPtr::from(&b11)), "Counter", "main")
            .await
            .unwrap(),
        Some(json!({"value": "975"}))
    );
    store.close().await;
    let store = db
        .open_storage(APPLICATION_SQL, CounterStorage { fail_revert: false })
        .await;
    store.revert_block(&b11).await.unwrap();
    assert_eq!(db.count("counter").await, 1);
    assert_eq!(store.block_ptr().await.unwrap(), Some(BlockPtr::from(&b10)));
    store.close().await;
    db.cleanup().await;
}
