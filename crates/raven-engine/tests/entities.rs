use std::collections::BTreeMap;

use async_trait::async_trait;
use raven_engine::{
    Entity, EntityError, EntityStore, EntityStoreExt, EntityValue, RavenError, RavenResult,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Balance {
    id: String,
    amount: u64,
}

impl Entity for Balance {
    const ENTITY_NAME: &'static str = "Balance";

    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Default)]
struct MemoryStore(BTreeMap<(String, String), EntityValue>);

#[async_trait]
impl EntityStore for MemoryStore {
    async fn get(&mut self, kind: &str, id: &str) -> RavenResult<Option<EntityValue>> {
        Ok(self.0.get(&(kind.to_owned(), id.to_owned())).cloned())
    }

    async fn put(&mut self, kind: &str, id: &str, data: &EntityValue) -> RavenResult<()> {
        self.0
            .insert((kind.to_owned(), id.to_owned()), data.clone());
        Ok(())
    }

    async fn delete(&mut self, kind: &str, id: &str) -> RavenResult<()> {
        self.0.remove(&(kind.to_owned(), id.to_owned()));
        Ok(())
    }
}

#[tokio::test]
async fn typed_access_uses_the_raw_store_through_a_trait_object() {
    let mut memory = MemoryStore::default();
    let store: &mut dyn EntityStore = &mut memory;
    assert_eq!(store.load::<Balance>("alice").await.unwrap(), None);
    let balance = Balance {
        id: "alice".to_owned(),
        amount: 10,
    };
    store.save(&balance).await.unwrap();
    assert_eq!(
        store.get("Balance", "alice").await.unwrap(),
        Some(json!({"id": "alice", "amount": 10}))
    );
    assert_eq!(store.load::<Balance>("alice").await.unwrap(), Some(balance));
    store
        .put("Balance", "alice", &json!({"id": "alice", "amount": 20}))
        .await
        .unwrap();
    assert_eq!(
        store
            .load::<Balance>("alice")
            .await
            .unwrap()
            .unwrap()
            .amount,
        20
    );
    store.remove::<Balance>("alice").await.unwrap();
    assert_eq!(store.get("Balance", "alice").await.unwrap(), None);
}

#[tokio::test]
async fn invalid_entities_and_mismatched_ids_are_errors_instead_of_missing_values() {
    let mut store = MemoryStore::default();
    store
        .put(
            "Balance",
            "alice",
            &json!({"id": "alice", "amount": "invalid"}),
        )
        .await
        .unwrap();
    assert!(matches!(
        store.load::<Balance>("alice").await,
        Err(RavenError::Entity(EntityError::Decode {
            entity_type: "Balance",
            ..
        }))
    ));
    store
        .put("Balance", "alice", &json!({"id": "bob", "amount": 10}))
        .await
        .unwrap();
    assert!(
        matches!(store.load::<Balance>("alice").await, Err(RavenError::Entity(EntityError::IdMismatch { expected, actual, .. })) if expected == "alice" && actual == "bob")
    );
}
