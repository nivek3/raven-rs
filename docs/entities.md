# Entities

Entities are application values used while handlers process one block. Raven does not turn an entity into a predefined table. An application chooses the database mapping and history model behind it.

## Typed entity access

Implement `Entity` on a serde value. `ENTITY_NAME` is a stable application storage type name; `id()` selects one instance of that type. Keep the name unique within an index and aligned with the storage adapter's mapping.

```rust,ignore
use raven_engine::{Entity, EntityStore, EntityStoreExt, RavenResult};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Balance { id: String, amount: String }

impl Entity for Balance {
    const ENTITY_NAME: &'static str = "Balance";
    fn id(&self) -> &str { &self.id }
}

async fn update_balance(entities: &mut dyn EntityStore, id: &str) -> RavenResult<()> {
    let mut balance = entities.load::<Balance>(id).await?
        .unwrap_or_else(|| Balance { id: id.to_owned(), amount: "0".to_owned() });
    balance.amount = "100".to_owned();
    entities.save(&balance).await?;
    Ok(())
}
```

Import `EntityStoreExt` to use `load::<T>(id)`, `save(&entity)`, and `remove::<T>(id)`. `load` returns `None` if absent; JSON decode failures and IDs that disagree with the lookup are errors. `save` stages a complete replacement under `T::ENTITY_NAME` and `entity.id()`.

Typed and raw access share one block-local state. The raw `EntityStore` API uses `EntityValue`, Raven's name for `serde_json::Value`, with `get(entity_type, id)`, `put(entity_type, id, value)`, and `delete(entity_type, id)`. Normal handlers should prefer typed entities; raw access is for dynamic entity kinds or values.

## Entity changes are not a schema

At commit time Raven gives storage the final `EntityChange` values for the block. The storage adapter decides how those values become SQL: it may update a current table, close a historical range and insert a successor, or record a native immutable event. An entity name is therefore a mapping key, not a database table contract.

The [ERC20 entities](https://github.com/nivek3/raven-rs/blob/main/examples/erc20/src/entities.rs) and [handlers](https://github.com/nivek3/raven-rs/blob/main/examples/erc20/src/handlers.rs) show typed reads and replacements for metadata, balances, transfers, and transactions.
