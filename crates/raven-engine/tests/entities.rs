use std::{borrow::Cow, collections::BTreeMap};

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

    fn id(&self) -> Cow<'_, str> {
        self.id.as_str().into()
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct NumericEvent {
    id: u128,
}

impl Entity for NumericEvent {
    const ENTITY_NAME: &'static str = "NumericEvent";

    fn id(&self) -> Cow<'_, str> {
        self.id.to_string().into()
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
async fn numeric_ids_round_trip_without_precision_loss_and_validate_storage_keys() {
    let mut store = MemoryStore::default();
    let event = NumericEvent { id: u128::MAX };
    let id = event.id().into_owned();
    store.save(&event).await.unwrap();
    let stored = store.get("NumericEvent", &id).await.unwrap().unwrap();
    assert!(stored["id"].is_number());
    assert_eq!(
        stored["id"].as_number().and_then(|id| id.as_u128()),
        Some(u128::MAX)
    );
    assert_eq!(store.load::<NumericEvent>(&id).await.unwrap(), Some(event));
    store
        .put("NumericEvent", "1", &json!({"id": 2}))
        .await
        .unwrap();
    assert!(matches!(
        store.load::<NumericEvent>("1").await,
        Err(RavenError::Entity(EntityError::IdMismatch { expected, actual, .. }))
            if expected == "1" && actual == "2"
    ));
    for invalid in ["-1", "340282366920938463463374607431768211456"] {
        let value = serde_json::from_str(&format!("{{\"id\":{invalid}}}")).unwrap();
        assert!(serde_json::from_value::<NumericEvent>(value).is_err());
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
