//! Block-scoped entity state used while handlers run outside the write transaction.

use std::collections::BTreeMap;

use async_trait::async_trait;

use serde::{Serialize, de::DeserializeOwned};

use crate::{BlockPtr, ChainStore, EngineError, EntityError, RavenResult};

pub use serde_json::Value as EntityValue;

/// An application entity with a stable storage type name and instance ID.
/// Type names must be unique within an index and match the application's storage
/// mapping. Serialization defines the value passed to its storage adapter.
pub trait Entity: Serialize + DeserializeOwned + Send + Sync {
    /// Stable name used by the application's storage mapping.
    const ENTITY_NAME: &'static str;

    /// ID under which this instance is stored.
    fn id(&self) -> &str;
}

/// Typed entity access over the same block-local state as `EntityStore`.
/// Import this trait to load, save and remove application entities, including
/// through `&mut dyn EntityStore`. Saving replaces the entire entity value.
#[async_trait]
pub trait EntityStoreExt: EntityStore {
    /// Loads and decodes an entity, checking that its ID matches the lookup.
    async fn load<T: Entity>(&mut self, id: &str) -> RavenResult<Option<T>> {
        let Some(value) = self.get(T::ENTITY_NAME, id).await? else {
            return Ok(None);
        };
        let entity: T = serde_json::from_value(value).map_err(|source| EntityError::Decode {
            entity_type: T::ENTITY_NAME,
            source,
        })?;
        if entity.id() != id {
            return Err(EntityError::IdMismatch {
                entity_type: T::ENTITY_NAME,
                expected: id.to_owned(),
                actual: entity.id().to_owned(),
            }
            .into());
        }
        Ok(Some(entity))
    }

    /// Serializes and stages an entity under its declared type name and ID.
    async fn save<T: Entity>(&mut self, entity: &T) -> RavenResult<()> {
        let value = serde_json::to_value(entity).map_err(|source| EntityError::Encode {
            entity_type: T::ENTITY_NAME,
            source,
        })?;
        self.put(T::ENTITY_NAME, entity.id(), &value).await
    }

    /// Stages deletion of an entity of the selected type.
    async fn remove<T: Entity>(&mut self, id: &str) -> RavenResult<()> {
        self.delete(T::ENTITY_NAME, id).await
    }
}

impl<S: EntityStore + ?Sized> EntityStoreExt for S {}

/// Final state of one entity after processing a block. None deletes the entity;
/// Some(EntityValue::Null) stores a JSON null value rather than deleting it.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityChange {
    pub entity_type: String,
    pub entity_id: String,
    pub data: Option<EntityValue>,
}

/// Restricted state access for one block. Reads observe earlier writes from
/// the same block. Persistent backends must preserve previous state for rollback;
/// handlers see application JSON only and cannot commit through this interface.
#[async_trait]
pub trait EntityStore: Send {
    /// Reads the value visible to the current block, including earlier pending writes.
    async fn get(&mut self, entity_type: &str, id: &str) -> RavenResult<Option<EntityValue>>;
    /// Stages a JSON value for publication when the current block commits.
    async fn put(&mut self, entity_type: &str, id: &str, data: &EntityValue) -> RavenResult<()>;
    /// Stages removal of an entity when the current block commits.
    async fn delete(&mut self, entity_type: &str, id: &str) -> RavenResult<()>;
}

/// Reads committed entities lazily and keeps handler writes in memory. Later
/// handlers observe earlier writes and deletions, including cached missing values.
/// Dropping the state discards the entire block's pending changes.
pub(crate) struct EntityState<'a, S: ChainStore> {
    store: &'a S,
    expected: Option<BlockPtr<S::Hash>>,
    loaded: BTreeMap<(String, String), Option<EntityValue>>,
    changes: BTreeMap<(String, String), Option<EntityValue>>,
    failed: bool,
}

impl<'a, S: ChainStore> EntityState<'a, S> {
    /// Creates an empty block-local cache tied to the expected committed pointer.
    pub(crate) fn new(store: &'a S, expected: Option<BlockPtr<S::Hash>>) -> Self {
        Self {
            store,
            expected,
            loaded: BTreeMap::new(),
            changes: BTreeMap::new(),
            failed: false,
        }
    }

    /// Rejects further use after a failed committed-state read.
    fn ensure_not_failed(&self) -> RavenResult<()> {
        if self.failed {
            return Err(EngineError::EntityStateFailed.into());
        }
        Ok(())
    }

    /// Returns the staged entity changes, unless an earlier read invalidated this state.
    pub(crate) fn into_changes(self) -> RavenResult<Vec<EntityChange>> {
        self.ensure_not_failed()?;
        Ok(self
            .changes
            .into_iter()
            .map(|((entity_type, entity_id), data)| EntityChange {
                entity_type,
                entity_id,
                data,
            })
            .collect())
    }
}

#[async_trait]
impl<S: ChainStore> EntityStore for EntityState<'_, S> {
    /// Returns a staged value or loads and caches the committed value on first access.
    async fn get(&mut self, entity_type: &str, id: &str) -> RavenResult<Option<EntityValue>> {
        self.ensure_not_failed()?;
        let key = (entity_type.to_owned(), id.to_owned());
        if let Some(data) = self.changes.get(&key).or_else(|| self.loaded.get(&key)) {
            return Ok(data.clone());
        }
        // A failed or cancelled load invalidates the state even if a handler catches
        // the error, so incomplete execution cannot later be committed.
        self.failed = true;
        let data = self
            .store
            .get_entity(self.expected.as_ref(), entity_type, id)
            .await?;
        self.loaded.insert(key, data.clone());
        self.failed = false;
        Ok(data)
    }

    /// Records a replacement value that later reads in this block will observe.
    async fn put(&mut self, entity_type: &str, id: &str, data: &EntityValue) -> RavenResult<()> {
        self.ensure_not_failed()?;
        self.changes
            .insert((entity_type.to_owned(), id.to_owned()), Some(data.clone()));
        Ok(())
    }

    /// Records a deletion that later reads in this block will observe.
    async fn delete(&mut self, entity_type: &str, id: &str) -> RavenResult<()> {
        self.ensure_not_failed()?;
        self.changes
            .insert((entity_type.to_owned(), id.to_owned()), None);
        Ok(())
    }
}
