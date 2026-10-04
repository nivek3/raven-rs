# PostgreSQL storage

`PostgresChainStore` owns canonical block metadata, indexing progress and one
writer session per dedicated schema. It coordinates application writes and
rollback in the same PostgreSQL transaction as progress.

The framework manages `blocks` and `networks`. Applications define entity
relations, columns, serialization, version history and rollback SQL.

## Application storage

Implement [PostgresStorage](src/storage.rs) with three operations:

- `get_entity`: read the current entity from application tables.
- `apply_block`: apply one block's final changes, preserving required versions.
- `revert_block`: undo the current head using persisted application versions.

Each operation receives Raven's `PgConnection` with the index schema selected.
Use that connection for all SQL. Do not commit, roll back, change session settings
or publish external side effects inside these operations.

Open a store with trusted application SQL and its storage implementation:

```rust,ignore
let store = PostgresChainStore::<B256, ChainIdentity>::connect_with_schema_sql(
    &database_url, "vault_index", "ethereum-mainnet",
    include_str!("schema.sql"), VaultStorage,
).await?;
```

For already-installed tables, `connect(url, schema, network_name, storage)` opens
without extra application DDL. SQL initialization runs after writer ownership is
acquired and in the same transaction as core DDL. Application SQL must be safe to
execute on restart. Applications supply their schema and manage its changes.

Network metadata uses `chain_id` (`NUMERIC(20,0)`, full `u64`) and configured
`network_name` (`TEXT`). Block positions use `BIGINT`, up to `i64::MAX`.
Block status uses `SMALLINT`: `1 = canonical`, `0 = orphaned`. Core pointer,
continuity and writer checks run in Rust; the core schema has no SQL `CHECK`
constraints.

## Block processing and rollback

Handlers run outside the write transaction. The block-local entity state lazily
loads application entities and stages changes in memory; later events in the same
block observe earlier changes. Entities pass between handlers and storage as
serde/JSON values. Handler failure discards pending changes.

The store validates the expected pointer and continuity, records the block,
calls `apply_block`, then publishes progress and commits. On rollback it validates
the exact head, calls `revert_block`, marks the block orphaned and restores progress.
Application SQL errors or cancellation before commit undo both business writes
and metadata changes. Version history must survive a writer/process restart.

The [ERC20 schema](../../examples/erc20/src/schema.sql) uses `INT8RANGE`
versions for balances/accounts and `BIGINT` creation positions for immutable
entities. Update closes a version and inserts its successor; deletion only closes
a version. Rollback deletes versions created in the reverted block and reopens
previous ranges. Storage calls its versioning functions inside the writer
transaction. The
[Uniswap example](../../examples/uniswap-v3/src/schema.sql) implements the same
storage contract with different native event and counter tables.

## Application SQL queries

Applications and query services use ordinary SQLx pools or SQL clients. To make
several related queries observe one committed state, open a PostgreSQL
repeatable-read, read-only transaction in the application:

```rust,ignore
let pool = sqlx::PgPool::connect(&database_url).await?;
let mut tx = pool.begin().await?;
sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
    .execute(&mut *tx).await?;
sqlx::query("SET LOCAL search_path TO erc20_holders, pg_catalog")
    .execute(&mut *tx).await?;
let progress: Option<i64> = sqlx::query_scalar(
    "SELECT latest_block_number FROM networks WHERE singleton",
).fetch_one(&mut *tx).await?;
let balances: Vec<(String, String)> = sqlx::query_as(
    "SELECT id, value_exact::text FROM erc20_balance WHERE block_range @> $1",
).bind(10_i64).fetch_all(&mut *tx).await?;
// Related entity/progress reads use this same transaction.
tx.commit().await?;
```

Applications choose query isolation, pagination, filters and historical block
positions. The [ERC20 balances_at function](../../examples/erc20/src/storage.rs)
shows a typed SQL query against versioned balances.

## Verification

Database tests require a dedicated `RAVEN_TEST_DATABASE_URL`. They create and
drop unique test schemas:

```sh
cargo test -p raven-postgres --test store --locked --offline -- --ignored
cargo test -p raven-postgres --test sql --locked --offline -- --ignored
```

Coverage includes writer ownership, expected pointers, cancellation, partial
write failure, failed application rollback, restart/replay, BIGINT positions and
application-owned read transactions. Entity version and historical query rules
are tested in the examples. See the [local acceptance workflow](../../examples/README.md#local-acceptance).
