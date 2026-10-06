# PostgreSQL storage

`PostgresChainStore` owns `blocks`, `networks` and one writer session per schema.
Application state and canonical progress commit or revert in one transaction.

## Writer connection

Use direct PostgreSQL or session pooling. Transaction pooling is unsupported:
ownership depends on a session advisory lock and backend PID. If the connection
is replaced, operations fail; close and reopen the store.

## Application storage

Implement [PostgresStorage](src/storage.rs): `get_entity` reads current values,
`apply_block` writes final changes/history, and `revert_block` restores the current
head using persisted history. Use only the supplied connection; do not manage its
transaction, change session settings or publish external side effects.

```rust,ignore
let store = PostgresChainStore::<B256, ChainIdentity>::connect_with_schema_sql(
    &database_url, "vault_index", "ethereum-mainnet",
    include_str!("schema.sql"), VaultStorage,
).await?;
```

Application DDL runs after writer acquisition, in the core initialization
transaction, and must be safe to rerun. `connect(url, schema, network_name, storage)`
opens without application DDL. Use a dedicated schema per index.

Core types: chain ID is `NUMERIC(20,0)` (full `u64`); positions are `BIGINT`
(up to `i64::MAX`); status is `SMALLINT` (`1` canonical, `0` orphaned).

## Block processing and rollback

Handlers stage changes before the write transaction. The store validates the
pointer and commits business writes, canonical metadata and progress together.
Application SQL failures roll back the transition. The application owns history
and rollback rules; see [Storage](../../docs/storage.md#history-model) and the
[ERC20 schema](../../examples/erc20/src/schema.sql).

## Application SQL queries

Use your own SQLx pool or SQL client. Related reads can share one snapshot:

```sql
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL search_path TO erc20_holders, pg_catalog;
SELECT latest_block_number FROM networks WHERE singleton;
SELECT id, value_exact FROM erc20_balance WHERE block_range @> 10::bigint;
COMMIT;
```

The [ERC20 balances_at helper](../../examples/erc20/src/storage.rs) provides a
typed historical query. Applications choose isolation, pagination and filters.

## Verification

See [database test commands](../../docs/testing.md#workspace-tests) and
[local acceptance](../../examples/README.md#local-acceptance). Database tests create
and drop isolated schemas and cover writer ownership, pointers, atomic failure,
cancellation, restart/replay and query snapshots.
