//! In-memory source and transactional state fixtures for integration tests.

// Independent test targets use different subsets of these shared fixtures.
#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::{Arc, Mutex, RwLock},
};

use async_trait::async_trait;

use raven_engine::{
    BlockBatch, BlockProcessor, BlockPtr, BlockSource, CancellationToken, ChainStore, Datasource,
    EngineError, EntityChange, EntityStore, EntityValue, LiteBlockHeader, Network, RavenError,
    RavenResult, Sender,
};

pub(crate) type Batch = BlockBatch<u64, u64>;

/// Builds a batch with the supplied header identity and fixture updates.
pub(crate) fn block(number: u64, hash: u64, parent_hash: u64, updates: Vec<u64>) -> Batch {
    BlockBatch {
        header: LiteBlockHeader {
            number,
            hash,
            parent_hash,
        },
        updates,
    }
}

#[derive(Clone)]
pub(crate) struct FakeSource {
    identity: u64,
    state: Arc<RwLock<SourceState>>,
}

#[derive(Default)]
struct SourceState {
    by_hash: BTreeMap<u64, Batch>,
    canonical: BTreeMap<u64, u64>,
    unavailable: BTreeSet<u64>,
    invalid: BTreeSet<u64>,
    switch_on_number: Option<Vec<Batch>>,
    hash_reads: Vec<u64>,
}

impl FakeSource {
    /// Creates an empty source fixture bound to the supplied chain identity.
    pub(crate) fn new(identity: u64) -> Self {
        Self {
            identity,
            state: Arc::new(RwLock::new(SourceState::default())),
        }
    }

    /// Makes exact-hash lookup return no batch for this hash.
    pub(crate) fn make_unavailable(&self, hash: u64) {
        self.state.write().unwrap().unavailable.insert(hash);
    }

    /// Makes source reads for this hash fail with an invalid-block error.
    pub(crate) fn reject_payload(&self, hash: u64) {
        self.state.write().unwrap().invalid.insert(hash);
    }

    /// Replaces the canonical branch on the next height-based lookup.
    pub(crate) fn switch_on_number_lookup(&self, branch: Vec<Batch>) {
        self.state.write().unwrap().switch_on_number = Some(branch);
    }

    /// Returns the hashes requested through exact batch lookup.
    pub(crate) fn hash_reads(&self) -> Vec<u64> {
        self.state.read().unwrap().hash_reads.clone()
    }

    /// Selects a complete canonical branch while retaining old blocks for hash lookup.
    pub(crate) fn set_chain(&self, blocks: Vec<Batch>) {
        assert!(
            blocks
                .windows(2)
                .all(|pair| pair[1].header.extends(&pair[0].header))
        );
        let mut state = self.state.write().unwrap();
        state.canonical.clear();
        for batch in blocks {
            if let Some(existing) = state.by_hash.get(&batch.header.hash) {
                assert_eq!(
                    existing, &batch,
                    "a hash cannot identify two fixture payloads"
                );
            }
            state
                .canonical
                .insert(batch.header.number, batch.header.hash);
            state.by_hash.insert(batch.header.hash, batch);
        }
    }
}

#[async_trait]
impl BlockSource for FakeSource {
    type Hash = u64;
    type Update = u64;
    type ChainIdentity = u64;

    /// Returns the source fixture's configured chain identity.
    async fn chain_identity(&self) -> RavenResult<u64> {
        Ok(self.identity)
    }

    /// Returns the last header on the selected canonical branch.
    async fn head(&self) -> RavenResult<LiteBlockHeader<u64>> {
        let state = self.state.read().unwrap();
        let hash = state
            .canonical
            .last_key_value()
            .map(|(_, hash)| hash)
            .ok_or_else(|| {
                RavenError::Source(Box::new(io::Error::other("fixture head unavailable")))
            })?;
        Ok(state.by_hash[hash].header.clone())
    }

    /// Returns the canonical batch at this height, applying a scheduled branch switch first.
    async fn block_by_number(&self, number: u64) -> RavenResult<Option<Batch>> {
        let branch = self.state.write().unwrap().switch_on_number.take();
        if let Some(branch) = branch {
            self.set_chain(branch);
        }
        let state = self.state.read().unwrap();
        let Some(hash) = state.canonical.get(&number) else {
            return Ok(None);
        };
        if state.invalid.contains(hash) {
            return Err(EngineError::InvalidBlock.into());
        }
        Ok(Some(state.by_hash[hash].clone()))
    }

    /// Returns the retained batch for this exact hash and records the lookup.
    async fn block_by_hash(&self, hash: &u64) -> RavenResult<Option<Batch>> {
        let mut state = self.state.write().unwrap();
        state.hash_reads.push(*hash);
        if state.unavailable.contains(hash) {
            return Ok(None);
        }
        if state.invalid.contains(hash) {
            return Err(EngineError::InvalidBlock.into());
        }
        Ok(state.by_hash.get(hash).cloned())
    }
}

#[async_trait]
impl Datasource for FakeSource {
    type Hash = u64;
    type Update = u64;

    /// Emits a snapshot of canonical batches from the requested height until cancellation or EOF.
    async fn consume(
        &self,
        next_block: u64,
        sender: Sender<Batch>,
        cancellation: CancellationToken,
    ) -> RavenResult<()> {
        // Snapshot one producer generation. A later branch switch leaves stale queued
        // batches intentionally available for engine resynchronization tests.
        let batches: RavenResult<Vec<_>> = {
            let state = self.state.read().unwrap();
            state
                .canonical
                .range(next_block..)
                .map(|(_, hash)| {
                    if state.invalid.contains(hash) {
                        Err(EngineError::InvalidBlock.into())
                    } else {
                        Ok(state.by_hash[hash].clone())
                    }
                })
                .collect()
        };
        let batches = batches?;
        for batch in batches {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                sent = sender.send(batch) => {
                    sent.map_err(|error| RavenError::Source(Box::new(error)))?;
                }
            }
        }
        Ok(())
    }
}

/// In-memory transactional fixture with a journal of complete pre-block values.
/// This models correctness for tests; it is not a persistent backend.
#[derive(Debug, Clone, Default)]
pub(crate) struct FakeStore {
    state: Arc<Mutex<StoreState>>,
}

#[derive(Debug, Default)]
struct StoreState {
    network: Option<Network<u64, u64>>,
    headers: BTreeMap<u64, LiteBlockHeader<u64>>,
    entities: BTreeMap<(String, String), FakeEntity>,
    before: BTreeMap<u64, BTreeMap<(String, String), FakeEntity>>,
    operations: Vec<(bool, u64)>,
    fail_commit: Option<u64>,
    fail_revert: Option<u64>,
    fail_entity_read: bool,
    begun_blocks: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct FakeEntity {
    data: EntityValue,
    updated_block_hash: u64,
}

impl StoreState {
    /// Derives the current pointer from the highest committed fixture header.
    fn block_ptr(&self) -> Option<BlockPtr<u64>> {
        self.headers
            .last_key_value()
            .map(|(_, header)| BlockPtr::from(header))
    }
}

impl FakeStore {
    /// Creates a store fixture with immutable network metadata and empty indexed state.
    pub(crate) fn new(network: Network<u64, u64>) -> Self {
        let state = StoreState {
            network: Some(network),
            ..StoreState::default()
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// Returns the fixture's persisted network metadata.
    pub(crate) fn network(&self) -> Option<Network<u64, u64>> {
        self.state.lock().unwrap().network.clone()
    }

    /// Returns the pointer of the highest committed fixture block.
    pub(crate) fn block_ptr(&self) -> Option<BlockPtr<u64>> {
        self.state.lock().unwrap().block_ptr()
    }

    /// Returns the locally committed header at this height.
    pub(crate) fn block_header_by_number(&self, number: u64) -> Option<LiteBlockHeader<u64>> {
        self.state.lock().unwrap().headers.get(&number).cloned()
    }

    /// Removes one committed header to simulate incomplete local history.
    pub(crate) fn forget_header(&self, number: u64) {
        self.state.lock().unwrap().headers.remove(&number);
    }

    /// Decodes the accumulated fixture updates from the shared values entity.
    pub(crate) fn values(&self) -> Vec<u64> {
        self.entity("Fixture", "values")
            .map(|data| serde_json::from_value(data).unwrap())
            .unwrap_or_default()
    }
    /// Returns the current JSON value for one fixture entity.
    pub(crate) fn entity(&self, entity_type: &str, id: &str) -> Option<EntityValue> {
        self.state
            .lock()
            .unwrap()
            .entities
            .get(&(entity_type.into(), id.into()))
            .map(|entity| entity.data.clone())
    }
    /// Returns the block hash that last wrote one fixture entity.
    pub(crate) fn entity_block_hash(&self, entity_type: &str, id: &str) -> Option<u64> {
        self.state
            .lock()
            .unwrap()
            .entities
            .get(&(entity_type.into(), id.into()))
            .map(|entity| entity.updated_block_hash)
    }
    /// Returns the number of commit attempts that reached the write phase.
    pub(crate) fn begun_blocks(&self) -> usize {
        self.state.lock().unwrap().begun_blocks
    }
    /// Enables or clears the injected committed-entity read failure.
    pub(crate) fn fail_entity_read(&self, fail: bool) {
        self.state.lock().unwrap().fail_entity_read = fail;
    }
    /// Returns the recorded apply and revert operations in execution order.
    pub(crate) fn operations(&self) -> Vec<(bool, u64)> {
        self.state.lock().unwrap().operations.clone()
    }
    /// Configures the block hash whose commit attempt must fail.
    pub(crate) fn fail_commit(&self, hash: Option<u64>) {
        self.state.lock().unwrap().fail_commit = hash;
    }
    /// Configures the block hash whose revert attempt must fail.
    pub(crate) fn fail_revert(&self, hash: Option<u64>) {
        self.state.lock().unwrap().fail_revert = hash;
    }

    /// Validates continuity, journals pre-block entities, and commits all fixture changes together.
    fn commit_expected(
        &self,
        expected: Option<&BlockPtr<u64>>,
        header: &LiteBlockHeader<u64>,
        changes: Vec<EntityChange>,
    ) -> RavenResult<()> {
        let mut state = self.state.lock().unwrap();
        if state.block_ptr().as_ref() != expected {
            return Err(EngineError::BlockPtrConflict.into());
        }
        let network = state
            .network
            .as_ref()
            .ok_or(raven_engine::PositionError::MissingNetwork)?;
        let valid = match state.headers.last_key_value() {
            Some((_, previous)) => header.extends(previous),
            None => network.accepts_first(header),
        };
        if !valid {
            return Err(EngineError::InvalidBlock.into());
        }
        state.begun_blocks += 1;
        if state.fail_commit == Some(header.hash) {
            return Err(store_failure());
        }
        let mut entities = state.entities.clone();
        for change in changes {
            let key = (change.entity_type, change.entity_id);
            match change.data {
                Some(data) => {
                    entities.insert(
                        key,
                        FakeEntity {
                            data,
                            updated_block_hash: header.hash,
                        },
                    );
                }
                None => {
                    entities.remove(&key);
                }
            }
        }
        let before = state.entities.clone();
        state.before.insert(header.hash, before);
        state.entities = entities;
        state.operations.push((true, header.hash));
        state.headers.insert(header.number, header.clone());
        Ok(())
    }

    /// Commits a contiguous header without changing fixture entities.
    pub(crate) fn commit_empty_block(&self, header: LiteBlockHeader<u64>) -> RavenResult<()> {
        self.commit_expected(self.block_ptr().as_ref(), &header, Vec::new())
    }

    /// Restores the journaled entity snapshot when the supplied header is the local head.
    pub(crate) fn revert_block(&self, header: &LiteBlockHeader<u64>) -> RavenResult<()> {
        let mut state = self.state.lock().unwrap();
        if state.headers.last_key_value().map(|(_, head)| head) != Some(header) {
            return Err(EngineError::BlockPtrConflict.into());
        }
        if state.fail_revert == Some(header.hash) {
            return Err(store_failure());
        }
        let before = state
            .before
            .remove(&header.hash)
            .ok_or(EngineError::InvalidLocalHistory)?;
        state.entities = before;
        state.headers.remove(&header.number);
        state.operations.push((false, header.hash));
        Ok(())
    }
}

#[async_trait]
impl ChainStore for FakeStore {
    type Hash = u64;
    type ChainIdentity = u64;

    /// Returns the fixture's persisted network metadata through the store contract.
    async fn network(&self) -> RavenResult<Option<Network<u64, u64>>> {
        Ok(self.network())
    }

    /// Persists matching network metadata only when local state is otherwise empty.
    async fn initialize(&self, network: &Network<u64, u64>) -> RavenResult<()> {
        let mut state = self.state.lock().unwrap();
        if let Some(existing) = &state.network {
            return if existing == network {
                Ok(())
            } else {
                Err(EngineError::BlockPtrConflict.into())
            };
        }
        if !state.headers.is_empty() || !state.entities.is_empty() {
            return Err(EngineError::InvalidLocalHistory.into());
        }
        state.network = Some(network.clone());
        Ok(())
    }

    /// Returns the current fixture pointer through the store contract.
    async fn block_ptr(&self) -> RavenResult<Option<BlockPtr<u64>>> {
        Ok(self.block_ptr())
    }
    /// Looks up a locally committed header through the store contract.
    async fn block_header_by_number(
        &self,
        number: u64,
    ) -> RavenResult<Option<LiteBlockHeader<u64>>> {
        Ok(self.block_header_by_number(number))
    }
    /// Reads an entity only if the caller's expected pointer still matches the fixture.
    async fn get_entity(
        &self,
        expected: Option<&BlockPtr<u64>>,
        entity_type: &str,
        id: &str,
    ) -> RavenResult<Option<EntityValue>> {
        let state = self.state.lock().unwrap();
        if state.block_ptr().as_ref() != expected {
            return Err(EngineError::BlockPtrConflict.into());
        }
        if state.fail_entity_read {
            return Err(store_failure());
        }
        Ok(state
            .entities
            .get(&(entity_type.into(), id.into()))
            .map(|entity| entity.data.clone()))
    }
    /// Delegates the contract commit to the fixture's atomic validation and journaling path.
    async fn commit_block(
        &self,
        expected: Option<&BlockPtr<u64>>,
        header: &LiteBlockHeader<u64>,
        changes: Vec<EntityChange>,
    ) -> RavenResult<()> {
        self.commit_expected(expected, header, changes)
    }
    /// Delegates head-only rollback to the fixture's journal restoration path.
    async fn revert_block(&self, header: &LiteBlockHeader<u64>) -> RavenResult<()> {
        self.revert_block(header)
    }
}

#[derive(Default)]
pub(crate) struct FakeProcessor {
    pub(crate) fail_on: Option<u64>,
}

#[async_trait]
impl BlockProcessor<u64, u64> for FakeProcessor {
    /// Appends batch updates to the shared values entity and optionally fails after staging them.
    async fn process(&self, entities: &mut dyn EntityStore, batch: &Batch) -> RavenResult<()> {
        let mut values: Vec<u64> = entities
            .get("Fixture", "values")
            .await?
            .map(|data| serde_json::from_value(data).unwrap())
            .unwrap_or_default();
        values.extend_from_slice(&batch.updates);
        entities
            .put("Fixture", "values", &serde_json::json!(values))
            .await?;
        if self.fail_on == Some(batch.header.hash) {
            return Err(RavenError::Processor(Box::new(io::Error::other(
                "fixture processor failure",
            ))));
        }
        Ok(())
    }
}

/// Builds the injected chain-store failure used by fixture operations.
fn store_failure() -> RavenError {
    RavenError::ChainStore(Box::new(io::Error::other("fixture store failure")))
}
