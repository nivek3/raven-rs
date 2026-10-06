# Storage

Raven owns `blocks`, `networks` and canonical progress. Applications own business
tables, SQL reads, version history and rollback.

## The application storage contract

Implement `PostgresStorage` using Raven's supplied `PgConnection`:

| Method | Responsibility |
| --- | --- |
| `get_entity` | Read the current committed value; do not write |
| `apply_block` | Apply each entity's final change and retain rollback history; an empty slice still indexes a block |
| `revert_block` | Undo exactly the current head, including after a process restart |

Use that connection for all operations. Do not commit, roll back, change session
settings or publish external side effects inside these methods. See the
[PostgreSQL package](https://github.com/nivek3/raven-rs/blob/main/crates/raven-postgres/README.md#application-storage) for construction.

## One transaction, two responsibilities

The store validates the expected pointer, applies application writes and updates
canonical metadata/progress in one transaction. Reverting a block restores both
in one transaction. Application SQL failures roll back the transition; Raven does
not infer business rollback rules.

## History model

The examples version mutable rows with half-open ranges `[start, end)`. Updates
close the current version and insert its successor; deletion only closes it.
Rollback deletes versions born at the reverted height and reopens predecessors.
Immutable events are deleted by creation block.

This model is optional. See the
[ERC20 schema](https://github.com/nivek3/raven-rs/blob/main/examples/erc20/src/schema.sql).
For related queries to observe one committed state, use an application-owned
repeatable-read, read-only transaction.

## Writer connection

Use a direct PostgreSQL connection or session pooling. Transaction pooling is
unsupported: ownership depends on a session advisory lock and backend PID.
If the connection is replaced, operations fail; close and reopen the store.

## Schema ownership

`connect_with_schema_sql` runs trusted application DDL after acquiring writer
ownership, in the core initialization transaction. It must be safe to rerun.
Use a dedicated schema per index; changes to contracts or mapping semantics
require an application-managed migration or a fresh index.
