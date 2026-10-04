#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex, RwLock},
};

use alloy_consensus::{Signed, TxEnvelope, TxLegacy, transaction::Recovered};
use alloy_primitives::Signature;
use alloy_rpc_types_eth::Transaction;
use async_trait::async_trait;
use raven_engine::{
    BlockProcessor, BlockPtr, BlockSource, CancellationToken, ChainStore, Datasource, EngineError,
    EntityChange, EntityStore, EntityValue, Network, RavenError, RavenResult, Sender,
};
use raven_evm::{
    Address, B256, Block, BlockBatch, EvmError, EvmFilter, Filter, LiteBlockHeader, Log, LogUpdate,
    RpcLog, SolEvent, U256, Update, sol,
};

sol! {
    #[derive(Debug, PartialEq, Eq)]
    event Changed(address indexed account, uint256 value);
    #[derive(Debug, PartialEq, Eq)]
    event Notice(uint256 value) anonymous;
}

/// Creates a deterministic fixture hash from one byte.
pub fn hash(value: u8) -> B256 {
    B256::from([value; 32])
}
/// Creates a deterministic fixture address from one byte.
pub fn address(value: u8) -> Address {
    Address::from([value; 20])
}
/// Returns the canonical header shared by simple EVM fixtures.
pub fn header() -> LiteBlockHeader {
    LiteBlockHeader {
        number: 100,
        hash: hash(10),
        parent_hash: hash(9),
    }
}

/// Builds a full fixture block with two transaction bodies.
pub fn block() -> Block {
    let mut block: Block = Block::default();
    block.header.hash = hash(10);
    block.header.inner.number = 100;
    block.header.inner.parent_hash = hash(9);
    block.header.inner.timestamp = 1234;
    let transactions: Vec<_> = (0..2)
        .map(|index| {
            // Synthetic signed envelopes with fixed fixture hashes, not chain proofs.
            let signed = Signed::new_unchecked(
                TxLegacy::default(),
                Signature::new(U256::from(1), U256::from(2), false),
                hash(index as u8 + 1),
            );
            let envelope: TxEnvelope = signed.into();
            Transaction {
                inner: Recovered::new_unchecked(envelope, address(1)),
                block_hash: Some(hash(10)),
                block_number: Some(100),
                transaction_index: Some(index),
                effective_gas_price: None,
                block_timestamp: Some(1234),
            }
        })
        .collect();
    block.transactions = transactions.into();
    block
}

/// Builds a mined RPC log at the supplied transaction and log positions.
pub fn log(transaction_index: u64, log_index: u64) -> RpcLog {
    RpcLog {
        inner: Log {
            address: address(5),
            data: Changed {
                account: address(7),
                value: U256::from(42),
            }
            .encode_log_data(),
        },
        block_hash: Some(hash(10)),
        block_number: Some(100),
        block_timestamp: Some(1234),
        transaction_hash: Some(hash(transaction_index as u8 + 1)),
        transaction_index: Some(transaction_index),
        log_index: Some(log_index),
        removed: false,
    }
}

/// Requests full blocks and every log in fixture batches.
pub fn all() -> EvmFilter {
    EvmFilter {
        blocks: true,
        logs: Some(Filter::default()),
    }
}

/// Asserts that a result contains the expected EVM source error.
pub fn assert_error<T>(result: RavenResult<T>, expected: EvmError) {
    match result {
        Err(RavenError::Source(error)) => {
            assert_eq!(error.downcast_ref::<EvmError>(), Some(&expected))
        }
        _ => panic!("expected EVM source error: {expected}"),
    }
}

/// Extracts the synthetic value encoded in a fixture log update.
pub fn log_value(update: &Update) -> Option<u8> {
    match update {
        Update::Log(log) => Some(log.log.address.as_slice()[0]),
        Update::Block(_) => None,
    }
}

/// Builds a canonical log update carrying one synthetic value.
pub fn value_log(header: &LiteBlockHeader, value: u8, log_index: u64) -> LogUpdate {
    LogUpdate {
        log: Log::new_unchecked(address(value), Vec::new(), Default::default()),
        block_number: header.number,
        block_hash: header.hash,
        transaction_hash: hash(1),
        transaction_index: 0,
        log_index,
    }
}

/// Builds a block batch with synthetic logs for the supplied chain position.
pub fn chain_block(number: u64, hash_byte: u8, parent: u8, values: Vec<u8>) -> BlockBatch {
    let header = LiteBlockHeader {
        number,
        hash: hash(hash_byte),
        parent_hash: hash(parent),
    };
    let updates = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| Update::Log(value_log(&header, value, index as u64)))
        .collect();
    BlockBatch { header, updates }
}

#[derive(Clone)]
pub struct FakeSource {
    identity: u64,
    state: Arc<RwLock<SourceState>>,
    filter: Option<Arc<EvmFilter>>,
}

#[derive(Default)]
struct SourceState {
    by_hash: BTreeMap<B256, BlockBatch>,
    canonical: BTreeMap<u64, B256>,
    hash_reads: Vec<B256>,
}

impl FakeSource {
    /// Creates an empty fixture source for the supplied chain identity.
    pub fn new(identity: u64) -> Self {
        Self {
            identity,
            state: Arc::new(RwLock::new(SourceState::default())),
            filter: None,
        }
    }

    /// Sets the source-side filter applied to sequential and hash reads.
    pub fn with_filter(mut self, filter: EvmFilter) -> Self {
        self.filter = Some(Arc::new(filter));
        self
    }

    /// Projects a batch to the source filter while retaining its header.
    fn project(&self, mut batch: BlockBatch) -> BlockBatch {
        if let Some(filter) = &self.filter {
            batch.updates.retain(|update| match update {
                Update::Block(_) => filter.blocks,
                Update::Log(log) => filter
                    .logs
                    .as_ref()
                    .is_some_and(|filter| filter.matches(&log.log)),
            });
        }
        batch
    }

    /// Returns hashes requested through random-access reads.
    pub fn hash_reads(&self) -> Vec<B256> {
        self.state.read().unwrap().hash_reads.clone()
    }

    /// Replaces the fixture source's canonical chain.
    pub fn set_chain(&self, blocks: Vec<BlockBatch>) {
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
    type Hash = B256;
    type Update = Update;
    type ChainIdentity = u64;

    /// Returns the fixture chain identity.
    async fn chain_identity(&self) -> RavenResult<u64> {
        Ok(self.identity)
    }

    /// Returns the current canonical fixture head.
    async fn head(&self) -> RavenResult<LiteBlockHeader> {
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

    /// Reads a canonical fixture batch by height.
    async fn block_by_number(&self, number: u64) -> RavenResult<Option<BlockBatch>> {
        let state = self.state.read().unwrap();
        Ok(state
            .canonical
            .get(&number)
            .map(|hash| self.project(state.by_hash[hash].clone())))
    }

    /// Records and reads a fixture batch by hash.
    async fn block_by_hash(&self, hash: &B256) -> RavenResult<Option<BlockBatch>> {
        let mut state = self.state.write().unwrap();
        state.hash_reads.push(*hash);
        Ok(state
            .by_hash
            .get(hash)
            .cloned()
            .map(|batch| self.project(batch)))
    }
}

#[async_trait]
impl Datasource for FakeSource {
    type Hash = B256;
    type Update = Update;

    /// Sends canonical fixture batches from the requested height.
    async fn consume(
        &self,
        next_block: u64,
        sender: Sender<BlockBatch>,
        cancellation: CancellationToken,
    ) -> RavenResult<()> {
        let batches: Vec<_> = {
            let state = self.state.read().unwrap();
            state
                .canonical
                .range(next_block..)
                .map(|(_, hash)| self.project(state.by_hash[hash].clone()))
                .collect()
        };
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

#[derive(Debug, Clone, Default)]
pub struct FakeStore {
    state: Arc<Mutex<StoreState>>,
}

#[derive(Debug, Default)]
struct StoreState {
    network: Option<Network<B256, u64>>,
    headers: BTreeMap<u64, LiteBlockHeader>,
    entities: BTreeMap<(String, String), FakeEntity>,
    before: BTreeMap<B256, BTreeMap<(String, String), FakeEntity>>,
    operations: Vec<(bool, B256)>,
    fail_commit: Option<B256>,
    fail_entity_write: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct FakeEntity {
    data: EntityValue,
    updated_block_hash: B256,
}

impl StoreState {
    /// Returns the current fixture block pointer.
    fn block_ptr(&self) -> Option<BlockPtr<B256>> {
        self.headers
            .last_key_value()
            .map(|(_, header)| BlockPtr::from(header))
    }
}

impl FakeStore {
    /// Returns the initialized fixture network metadata.
    pub fn network(&self) -> Option<Network<B256, u64>> {
        self.state.lock().unwrap().network.clone()
    }

    /// Returns the current canonical fixture block pointer.
    pub fn block_ptr(&self) -> Option<BlockPtr<B256>> {
        self.state.lock().unwrap().block_ptr()
    }

    /// Returns values emitted by committed fixture logs.
    pub fn values(&self) -> Vec<u8> {
        self.entity("Fixture", "values")
            .map(|data| serde_json::from_value(data).unwrap())
            .unwrap_or_default()
    }

    /// Returns a stored fixture entity by type and identifier.
    pub fn entity(&self, entity_type: &str, id: &str) -> Option<EntityValue> {
        self.state
            .lock()
            .unwrap()
            .entities
            .get(&(entity_type.into(), id.into()))
            .map(|entity| entity.data.clone())
    }

    /// Returns the canonical block hash that last changed an entity.
    pub fn entity_block_hash(&self, entity_type: &str, id: &str) -> Option<B256> {
        self.state
            .lock()
            .unwrap()
            .entities
            .get(&(entity_type.into(), id.into()))
            .map(|entity| entity.updated_block_hash)
    }

    /// Enables or disables fixture failures during entity writes.
    pub fn fail_entity_write(&self, fail: bool) {
        self.state.lock().unwrap().fail_entity_write = fail;
    }

    /// Returns commit and rollback operations recorded by the fixture store.
    pub fn operations(&self) -> Vec<(bool, B256)> {
        self.state.lock().unwrap().operations.clone()
    }

    /// Configures a commit failure for one block hash, or clears it.
    pub fn fail_commit(&self, hash: Option<B256>) {
        self.state.lock().unwrap().fail_commit = hash;
    }

    /// Applies fixture changes after checking the expected block pointer.
    fn commit_expected(
        &self,
        expected: Option<&BlockPtr<B256>>,
        header: &LiteBlockHeader,
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
        if state.fail_commit == Some(header.hash) {
            return Err(store_failure());
        }
        let mut entities = state.entities.clone();
        for change in changes {
            if state.fail_entity_write {
                return Err(store_failure());
            }
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

    /// Reverts the current fixture head and restores its predecessor state.
    pub fn revert_block(&self, header: &LiteBlockHeader) -> RavenResult<()> {
        let mut state = self.state.lock().unwrap();
        if state.headers.last_key_value().map(|(_, head)| head) != Some(header) {
            return Err(EngineError::BlockPtrConflict.into());
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
    type Hash = B256;
    type ChainIdentity = u64;

    /// Returns the fixture network metadata.
    async fn network(&self) -> RavenResult<Option<Network<B256, u64>>> {
        Ok(self.network())
    }

    /// Initializes the empty fixture store with network metadata.
    async fn initialize(&self, network: &Network<B256, u64>) -> RavenResult<()> {
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

    /// Returns the current fixture block pointer.
    async fn block_ptr(&self) -> RavenResult<Option<BlockPtr<B256>>> {
        Ok(self.block_ptr())
    }

    /// Reads a fixture header at one canonical height.
    async fn block_header_by_number(&self, number: u64) -> RavenResult<Option<LiteBlockHeader>> {
        Ok(self.state.lock().unwrap().headers.get(&number).cloned())
    }

    /// Reads a fixture entity after checking the expected pointer.
    async fn get_entity(
        &self,
        expected: Option<&BlockPtr<B256>>,
        entity_type: &str,
        id: &str,
    ) -> RavenResult<Option<EntityValue>> {
        let state = self.state.lock().unwrap();
        if state.block_ptr().as_ref() != expected {
            return Err(EngineError::BlockPtrConflict.into());
        }
        Ok(state
            .entities
            .get(&(entity_type.into(), id.into()))
            .map(|entity| entity.data.clone()))
    }
    /// Commits fixture changes after continuity checks.
    async fn commit_block(
        &self,
        expected: Option<&BlockPtr<B256>>,
        header: &LiteBlockHeader,
        changes: Vec<EntityChange>,
    ) -> RavenResult<()> {
        self.commit_expected(expected, header, changes)
    }
    /// Reverts the current fixture head.
    async fn revert_block(&self, header: &LiteBlockHeader) -> RavenResult<()> {
        self.revert_block(header)
    }
}

pub struct FakeProcessor;

#[async_trait]
impl BlockProcessor<B256, Update> for FakeProcessor {
    /// Writes a fixed fixture value for every processed batch.
    async fn process(&self, entities: &mut dyn EntityStore, batch: &BlockBatch) -> RavenResult<()> {
        let mut values: Vec<u8> = entities
            .get("Fixture", "values")
            .await?
            .map(|data| serde_json::from_value(data).unwrap())
            .unwrap_or_default();
        values.extend(batch.updates.iter().filter_map(log_value));
        entities
            .put("Fixture", "values", &serde_json::json!(values))
            .await?;
        Ok(())
    }
}

/// Builds the chain-store error used by fixture failure paths.
fn store_failure() -> RavenError {
    RavenError::ChainStore(Box::new(io::Error::other("fixture store failure")))
}
