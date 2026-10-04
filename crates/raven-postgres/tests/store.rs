mod fake;

use fake::{Database, FixtureStorage, SCHEMA_SQL, TestStore, block, network};
use raven_engine::{BlockPtr, ChainStore, EntityChange, Network};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Row, types::Json};

impl Database {
    /// Opens a standard fixture store or fails the test.
    async fn open(&self) -> TestStore {
        self.try_open().await.unwrap()
    }

    /// Attempts to open the standard fixture store.
    async fn try_open(&self) -> raven_engine::RavenResult<TestStore> {
        TestStore::connect_with_schema_sql(
            &self.url,
            &self.schema,
            "test-chain",
            SCHEMA_SQL,
            FixtureStorage,
        )
        .await
    }

    /// Attempts to open a fixture store under the supplied network name.
    async fn try_open_with_network_name(&self, name: &str) -> raven_engine::RavenResult<TestStore> {
        TestStore::connect_with_schema_sql(
            &self.url,
            &self.schema,
            name,
            SCHEMA_SQL,
            FixtureStorage,
        )
        .await
    }

    /// Lists current fixture entities with their canonical update hashes.
    async fn entities(&self) -> Vec<(String, String, Value, Value)> {
        sqlx::query(AssertSqlSafe(format!(r#"SELECT e.entity_type, e.entity_id, e.data, b.hash AS updated_block_hash FROM "{}".test_entity e JOIN "{}".blocks b ON b.number = e.start_block AND b.status = 1 WHERE e.end_block IS NULL ORDER BY e.entity_type, e.entity_id"#, self.schema, self.schema)))
            .fetch_all(&self.pool).await.unwrap().into_iter().map(|r| (
                r.get("entity_type"), r.get("entity_id"), r.get::<Json<Value>, _>("data").0, r.get::<Json<Value>, _>("updated_block_hash").0,
            )).collect()
    }
}

/// Creates a change for the standard test entity.
fn change(id: &str, data: Option<Value>) -> EntityChange {
    EntityChange {
        entity_type: "Pool".into(),
        entity_id: id.into(),
        data,
    }
}

#[tokio::test]
/// Verifies that rejects invalid schema before connecting.
async fn rejects_invalid_schema_before_connecting() {
    for name in [
        "",
        "public",
        "pg_catalog",
        "has-dash",
        "UPPER",
        r#"a"; DROP SCHEMA public"#,
        "1bad",
    ] {
        assert!(
            TestStore::connect("unused", name, "test-chain", fake::FixtureStorage)
                .await
                .is_err()
        );
    }
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL; creates and drops a unique test schema"]
/// Verifies that networks persists and conflicting initialization is rejected.
async fn networks_persists_and_conflicting_initialization_is_rejected() {
    let db = Database::new().await;
    let store = db.open().await;
    assert_eq!(store.network().await.unwrap(), None);
    store.initialize(&network()).await.unwrap();
    store.initialize(&network()).await.unwrap();
    let wrong = Network::new(2, 10, Some(block(9, "b9", "b8"))).unwrap();
    assert!(store.initialize(&wrong).await.is_err());
    store.close().await;
    assert!(db.try_open_with_network_name("other-name").await.is_err());
    let store = db.open().await;
    assert_eq!(store.network().await.unwrap(), Some(network()));
    assert_eq!(store.block_ptr().await.unwrap(), None);
    store.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL"]
/// Verifies that block operations publish state and progress together.
async fn block_operations_publish_state_and_progress_together() {
    let db = Database::new().await;
    let store = db.open().await;
    store.initialize(&network()).await.unwrap();
    let b10 = block(10, "b10", "b9");
    store
        .commit_block(None, &b10, vec![change("a", Some(json!(1)))])
        .await
        .unwrap();
    assert_eq!(store.block_ptr().await.unwrap(), Some(BlockPtr::from(&b10)));
    assert_eq!(
        store
            .get_entity(Some(&BlockPtr::from(&b10)), "Pool", "a")
            .await
            .unwrap(),
        Some(json!(1))
    );
    assert_eq!(
        db.entities().await,
        vec![("Pool".into(), "a".into(), json!(1), json!("b10"))]
    );
    assert_eq!(db.count("test_entity").await, 1);
    assert_eq!(db.count("blocks").await, 1);
    // A stale submission must not overwrite committed state.
    assert!(
        store
            .commit_block(
                None,
                &block(11, "b11", "b10"),
                vec![change("a", Some(json!(99)))]
            )
            .await
            .is_err()
    );
    assert_eq!(
        store
            .get_entity(Some(&BlockPtr::from(&b10)), "Pool", "a")
            .await
            .unwrap(),
        Some(json!(1))
    );
    store.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL"]
/// Verifies that revert restores full images and same hash can be replayed.
async fn revert_restores_full_images_and_same_hash_can_be_replayed() {
    let db = Database::new().await;
    let store = db.open().await;
    store.initialize(&network()).await.unwrap();
    let b10 = block(10, "b10", "b9");
    let b11 = block(11, "b11", "b10");
    store
        .commit_block(
            None,
            &b10,
            vec![
                change("null", Some(json!(null))),
                change("a", Some(json!({"value": 1}))),
            ],
        )
        .await
        .unwrap();
    let initial = db.entities().await;
    for _ in 0..2 {
        store
            .commit_block(
                Some(&BlockPtr::from(&b10)),
                &b11,
                vec![
                    change("a", Some(json!(3))),
                    change("null", None),
                    change("new", Some(json!(null))),
                    change("absent", None),
                ],
            )
            .await
            .unwrap();
        let versions: Vec<(String, sqlx::types::Json<Value>)> = sqlx::query_as(AssertSqlSafe(format!(
            r#"SELECT entity_id, data FROM "{}".test_entity WHERE start_block = 11 ORDER BY entity_id"#, db.schema,
        ))).fetch_all(&db.pool).await.unwrap();
        assert_eq!(
            versions,
            vec![
                ("a".into(), sqlx::types::Json(json!(3))),
                ("new".into(), sqlx::types::Json(json!(null)))
            ]
        );
        let status: i16 = sqlx::query_scalar(AssertSqlSafe(format!(
            r#"SELECT status FROM "{}".blocks WHERE hash = $1"#,
            db.schema
        )))
        .bind(sqlx::types::Json(&b11.hash))
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(status, 1);
        assert_eq!(
            store
                .get_entity(Some(&BlockPtr::from(&b11)), "Pool", "null")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .get_entity(Some(&BlockPtr::from(&b11)), "Pool", "new")
                .await
                .unwrap(),
            Some(json!(null))
        );
        assert!(store.revert_block(&b10).await.is_err());
        store.revert_block(&b11).await.unwrap();
        let status: i16 = sqlx::query_scalar(AssertSqlSafe(format!(
            r#"SELECT status FROM "{}".blocks WHERE hash = $1"#,
            db.schema
        )))
        .bind(sqlx::types::Json(&b11.hash))
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(status, 0);
        assert_eq!(db.entities().await, initial);
        assert_eq!(db.count("test_entity").await, 2);
        assert_eq!(store.block_header_by_number(11).await.unwrap(), None);
        assert_eq!(store.block_ptr().await.unwrap(), Some(BlockPtr::from(&b10)));
    }
    store.revert_block(&b10).await.unwrap();
    assert!(db.entities().await.is_empty());
    assert_eq!(db.count("test_entity").await, 0);
    assert_eq!(store.block_ptr().await.unwrap(), None);
    assert_eq!(store.network().await.unwrap(), Some(network()));
    store.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL"]
/// Verifies that predecessor checks empty blocks and bigint heights.
async fn predecessor_checks_empty_blocks_and_bigint_heights() {
    let db = Database::new().await;
    let store = db.open().await;
    let height = i64::MAX as u64;
    let start = Network::new(
        u64::MAX,
        height,
        Some(block(height - 1, "parent", "grandparent")),
    )
    .unwrap();
    store.initialize(&start).await.unwrap();
    assert!(
        store
            .commit_block(None, &block(height, "last", "wrong"), Vec::new())
            .await
            .is_err()
    );
    let last = block(height, "last", "parent");
    store.commit_block(None, &last, Vec::new()).await.unwrap();
    assert_eq!(
        store.block_header_by_number(height).await.unwrap(),
        Some(last.clone())
    );
    assert!(store.commit_block(None, &last, Vec::new()).await.is_err());
    assert!(
        store
            .revert_block(&block(height, "last", "wrong"))
            .await
            .is_err()
    );
    store.revert_block(&last).await.unwrap();
    assert_eq!(store.network().await.unwrap(), Some(start));
    store.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL"]
/// Verifies that second writer is rejected and separate schemas are independent.
async fn second_writer_is_rejected_and_separate_schemas_are_independent() {
    let db = Database::new().await;
    let other = Database::new().await;
    let store = db.open().await;
    assert!(db.try_open().await.is_err());
    let independent = other.open().await;
    independent.initialize(&network()).await.unwrap();
    store.initialize(&network()).await.unwrap();
    independent.close().await;
    store.close().await;
    db.open().await.close().await;
    db.cleanup().await;
    other.cleanup().await;
}

#[tokio::test]
#[ignore = "requires RAVEN_TEST_DATABASE_URL with permission to terminate its own sessions"]
/// Verifies that lost writer connection cannot resume or commit.
async fn lost_writer_connection_cannot_resume_or_commit() {
    let db = Database::new().await;
    let store = db.open().await;
    store.initialize(&network()).await.unwrap();
    let b10 = block(10, "b10", "b9");
    let pid: i32 = sqlx::query_scalar("SELECT pid FROM pg_locks WHERE locktype = 'advisory' AND granted AND database = (SELECT oid FROM pg_database WHERE datname = current_database()) AND classid::bigint = (hashtext(current_database())::bigint & 4294967295) AND objid::bigint = (hashtext($1)::bigint & 4294967295) AND objsubid = 2")
        .bind(&db.schema).fetch_one(&db.pool).await.unwrap();
    let killed: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(killed);
    assert!(
        store
            .commit_block(None, &b10, vec![change("a", Some(json!(1)))])
            .await
            .is_err()
    );
    assert!(store.block_ptr().await.is_err());
    store.close().await;
    let reopened = db.open().await;
    assert_eq!(reopened.block_ptr().await.unwrap(), None);
    assert!(db.entities().await.is_empty());
    reopened.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL"]
/// Verifies that cancelled block submission rolls back internal transaction.
async fn cancelled_block_submission_rolls_back_internal_transaction() {
    let db = Database::new().await;
    let store = db.open().await;
    store.initialize(&network()).await.unwrap();
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query(AssertSqlSafe(format!(
        r#"LOCK TABLE "{}".test_entity IN ACCESS EXCLUSIVE MODE"#,
        db.schema
    )))
    .execute(&mut *blocker)
    .await
    .unwrap();
    let b10 = block(10, "b10", "b9");
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            store.commit_block(None, &b10, vec![change("a", Some(json!(1)))]),
        )
        .await
        .is_err()
    );
    blocker.rollback().await.unwrap();
    assert_eq!(store.block_ptr().await.unwrap(), None);
    assert_eq!(db.count("blocks").await, 0);
    assert_eq!(db.count("test_entity").await, 0);
    assert!(db.entities().await.is_empty());
    store.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL"]
/// Verifies that sql failure cannot publish partial block state.
async fn sql_failure_cannot_publish_partial_block_state() {
    let db = Database::new().await;
    let store = db.open().await;
    store.initialize(&network()).await.unwrap();
    sqlx::query(AssertSqlSafe(format!(
        r#"ALTER TABLE "{}".test_entity ALTER COLUMN entity_id TYPE VARCHAR(5)"#,
        db.schema
    )))
    .execute(&db.pool)
    .await
    .unwrap();
    assert!(
        store
            .commit_block(
                None,
                &block(10, "b10", "b9"),
                vec![
                    change("a", Some(json!(1))),
                    change("reject", Some(json!(2))),
                ]
            )
            .await
            .is_err()
    );
    assert_eq!(store.block_ptr().await.unwrap(), None);
    assert!(db.entities().await.is_empty());
    assert_eq!(db.count("test_entity").await, 0);
    assert_eq!(db.count("blocks").await, 0);
    store.close().await;
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires a dedicated RAVEN_TEST_DATABASE_URL"]
/// Verifies that genesis reorg and restart match clean replay.
async fn genesis_reorg_and_restart_match_clean_replay() {
    let db = Database::new().await;
    let clean = Database::new().await;
    let store = db.open().await;
    let fresh = clean.open().await;
    let start = Network::new(1, 0, None).unwrap();
    let genesis = block(0, "g0", "zero");
    let old = block(1, "old1", "g0");
    let new = block(1, "new1", "g0");
    for s in [&store, &fresh] {
        s.initialize(&start).await.unwrap();
        s.commit_block(None, &genesis, vec![change("volume", Some(json!(10)))])
            .await
            .unwrap();
    }
    store
        .commit_block(
            Some(&BlockPtr::from(&genesis)),
            &old,
            vec![
                change("volume", Some(json!(99))),
                change("orphan-only", Some(json!(true))),
            ],
        )
        .await
        .unwrap();
    store.close().await;
    let store = db.open().await;
    assert_eq!(store.block_ptr().await.unwrap(), Some(BlockPtr::from(&old)));
    store.revert_block(&old).await.unwrap();
    for s in [&store, &fresh] {
        let current = s
            .get_entity(Some(&BlockPtr::from(&genesis)), "Pool", "volume")
            .await
            .unwrap()
            .unwrap()
            .as_i64()
            .unwrap();
        s.commit_block(
            Some(&BlockPtr::from(&genesis)),
            &new,
            vec![change("volume", Some(json!(current + 5)))],
        )
        .await
        .unwrap();
    }
    assert_eq!(db.entities().await, clean.entities().await);
    assert_eq!(
        store.block_ptr().await.unwrap(),
        fresh.block_ptr().await.unwrap()
    );
    assert_eq!(
        store.block_header_by_number(1).await.unwrap(),
        fresh.block_header_by_number(1).await.unwrap()
    );
    assert_eq!(
        db.count("test_entity").await,
        clean.count("test_entity").await
    );
    store.revert_block(&new).await.unwrap();
    store.revert_block(&genesis).await.unwrap();
    assert_eq!(store.block_ptr().await.unwrap(), None);
    assert_eq!(store.network().await.unwrap(), Some(start));
    store.close().await;
    fresh.close().await;
    db.cleanup().await;
    clean.cleanup().await;
}
