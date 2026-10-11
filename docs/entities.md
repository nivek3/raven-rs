# Entities

Implement `Entity` for a serde value. Its unique, stable `ENTITY_NAME` selects the
application's storage mapping; `id()` selects an instance.
`id()` returns `Cow<'_, str>`: string IDs borrow their value, while numeric IDs
format a storage key. Raw store keys remain strings.

## Typed entity access

```rust,ignore
use raven_engine::{Entity, EntityStore, EntityStoreExt, RavenResult};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Serialize, Deserialize)]
struct Balance { id: String, amount: String }

impl Entity for Balance {
    const ENTITY_NAME: &'static str = "Balance";
    fn id(&self) -> Cow<'_, str> { self.id.as_str().into() }
}

async fn update_balance(entities: &mut dyn EntityStore, id: &str) -> RavenResult<()> {
    let mut balance = entities.load::<Balance>(id).await?
        .unwrap_or_else(|| Balance { id: id.to_owned(), amount: "0".to_owned() });
    balance.amount = "100".to_owned();
    entities.save(&balance).await?;
    Ok(())
}
```

| Method | Behavior |
| --- | --- |
| `load::<T>(id)` | Returns `None` if absent; errors on invalid JSON or mismatched IDs |
| `save(&entity)` | Stages a complete replacement |
| `remove::<T>(id)` | Stages deletion |

Typed access shares block-local state with raw `get/put/delete`, which use
`EntityValue` (`serde_json::Value`). Later handlers see earlier staged writes.

For a `u128` ID field, return `self.id.to_string().into()` from `id()`.
The workspace enables `serde_json`'s `arbitrary_precision` feature so the field
remains an exact JSON number. Native SQL tables can use `NUMERIC(39,0)` for IDs.

## Entity changes are not a schema

Storage receives each entity's final `EntityChange` for the block. Applications
own the SQL mapping, history and rollback; entity names do not prescribe tables.
See [Storage](storage.md) and the
[ERC20 entities](https://github.com/nivek3/raven-rs/blob/main/examples/erc20/src/entities.rs).
