# Storage

`ChainStore` persists Raven's network metadata, canonical block history, and committed pointer. `PostgresChainStore` also provides a PostgreSQL transaction to application storage. Raven owns the core `blocks` and `networks` relations; the application owns every business relation.

## The application storage contract

Implement `PostgresStorage` with three methods:

```rust,ignore
#[async_trait::async_trait]
impl PostgresStorage for Storage {
    async fn get_entity(&self, conn: &mut sqlx::PgConnection,
        entity_type: &str, id: &str) -> RavenResult<Option<EntityValue>> {
        /* application SQL */
    }

    async fn apply_block(&self, conn: &mut sqlx::PgConnection,
        block_number: u64, changes: &[EntityChange]) -> RavenResult<()> {
        /* apply current state and history */
    }

    async fn revert_block(&self, conn: &mut sqlx::PgConnection,
        block_number: u64) -> RavenResult<()> {
        /* restore state before this block */
    }
}
```

All three use Raven's supplied `PgConnection`. Do not commit, roll back, change session settings, or publish external side effects inside these methods.

`get_entity` reads the currently committed application value. `apply_block` receives only each entity's final value after all handlers for that block. An empty changes slice still represents an indexed empty block. `revert_block` must undo exactly the current head and work after a restart using persisted application history.

## One transaction, two responsibilities

On an apply, `PostgresChainStore` validates the expected pointer and continuity, records the canonical block, runs `apply_block`, updates progress, then commits. These operations share one transaction. On a revert, it validates that the target is the current head, runs `revert_block`, marks that header orphaned, restores progress, and commits.

So a successfully committed block has both business state and progress. An application SQL error or cancellation before commit rolls both back. Raven does not infer how to restore business state; schema and history rules are part of the application mapping.

## History model

The examples keep mutable rows in half-open block ranges `[start, end)`. An update closes the current version and inserts the successor. A deletion closes the current range. Reverting a block deletes versions born at that height and reopens their predecessors. Immutable event rows can instead retain their creation block and be deleted on rollback.

This is one viable model, not a framework requirement. The [ERC20 schema](https://github.com/nivek3/raven-rs/blob/main/examples/erc20/src/schema.sql) and [storage implementation](https://github.com/nivek3/raven-rs/blob/main/examples/erc20/src/storage.rs) show its full mapping. Read-side services choose their own joins, pagination, and isolation. If several queries must observe one logical projection, use a read-only repeatable-read transaction in that service.

## Schema ownership

`connect_with_schema_sql` installs application SQL after writer ownership is acquired and inside the core initialization transaction. The SQL must be safe to execute again at startup. Use a dedicated PostgreSQL schema per index and treat a change in indexed contracts or mapping semantics as an index migration or fresh-index decision made by the application.
