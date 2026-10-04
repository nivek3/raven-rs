#![allow(dead_code)]

use async_trait::async_trait;
use raven_engine::{RavenError, RavenResult};
use raven_evm::{Address, B256, Block, Log, RpcLog};
use rpc_block_crawler_datasource::{ProviderBuilder, RootProvider};
use serde_json::{Value, json};
use std::io;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::{AbortHandle, JoinSet},
};

/// Creates a deterministic hash fixture.
pub fn hash(value: u8) -> B256 {
    B256::from([value; 32])
}
/// Creates a deterministic address fixture.
pub fn address(value: u8) -> Address {
    Address::from([value; 20])
}
/// Creates a block fixture with a fixed transaction hash.
pub fn block(number: u64, id: u8, parent: u8) -> Block {
    let mut block: Block = Block::default();
    block.header.hash = hash(id);
    block.header.inner.number = number;
    block.header.inner.parent_hash = hash(parent);
    block.transactions = vec![hash(200)].into();
    block
}
/// Creates a log fixture bound to `number` and `block_id`.
pub fn log(number: u64, block_id: u8) -> RpcLog {
    RpcLog {
        inner: Log::new_unchecked(address(1), vec![hash(30)], Default::default()),
        block_number: Some(number),
        block_hash: Some(hash(block_id)),
        transaction_hash: Some(hash(200)),
        transaction_index: Some(0),
        log_index: Some(0),
        ..RpcLog::default()
    }
}
#[derive(Clone)]
pub struct FakeRpc {
    pub state: Arc<Mutex<State>>,
    endpoint: String,
    server: Arc<Server>,
}
struct Server(AbortHandle);
impl Drop for Server {
    /// Aborts the background JSON-RPC server task.
    fn drop(&mut self) {
        self.0.abort();
    }
}
#[derive(Clone)]
pub struct RpcCall {
    pub method: String,
    pub params: Value,
}
#[derive(Default)]
pub struct State {
    pub blocks: BTreeMap<B256, Block>,
    pub canonical: BTreeMap<u64, B256>,
    pub logs: BTreeMap<B256, Vec<RpcLog>>,
    pub calls: Vec<RpcCall>,
    pub fail_logs: bool,
    pub log_delay: std::time::Duration,
    pub pending: bool,
    pub switch_after_hash: Option<(B256, Vec<Block>)>,
    pub substitute_hash: Option<B256>,
}
impl State {
    /// Replaces the served canonical chain with `blocks`.
    fn set_chain(&mut self, blocks: Vec<Block>) {
        self.canonical.clear();
        for block in blocks {
            self.canonical
                .insert(block.header.inner.number, block.header.hash);
            self.blocks.insert(block.header.hash, block);
        }
    }
    /// Handles one JSON-RPC request and records it for assertions.
    fn response(&mut self, request: Value) -> Value {
        let call = RpcCall {
            method: request["method"].as_str().unwrap().to_owned(),
            params: request["params"].clone(),
        };
        self.calls.push(call.clone());
        let value = match call.method.as_str() {
            "eth_chainId" => json!("0x1"),
            "eth_getBlockByNumber" => {
                let tag = call.params[0].as_str().unwrap();
                let hash = if tag == "latest" {
                    self.canonical.last_key_value().map(|(_, hash)| hash)
                } else {
                    self.canonical
                        .get(&u64::from_str_radix(&tag[2..], 16).unwrap())
                };
                hash.map(|hash| json!(self.blocks[hash]))
                    .unwrap_or(Value::Null)
            }
            "eth_getBlockByHash" => {
                let hash: B256 = serde_json::from_value(call.params[0].clone()).unwrap();
                let selected = self.substitute_hash.unwrap_or(hash);
                let value = self
                    .blocks
                    .get(&selected)
                    .map(|block| json!(block))
                    .unwrap_or(Value::Null);
                if self
                    .switch_after_hash
                    .as_ref()
                    .is_some_and(|(trigger, _)| *trigger == hash)
                {
                    let (_, blocks) = self.switch_after_hash.take().unwrap();
                    self.set_chain(blocks);
                }
                value
            }
            "eth_getLogs" => {
                if self.fail_logs {
                    return json!({"jsonrpc":"2.0", "id":request["id"], "error":{"code":-32602,"message":"unsupported hash filter"}});
                }
                let query = &call.params[0];
                assert!(query.get("fromBlock").is_none() && query.get("toBlock").is_none());
                let hash: B256 = serde_json::from_value(query["blockHash"].clone()).unwrap();
                json!(self.logs.get(&hash).cloned().unwrap_or_default())
            }
            _ => panic!("unexpected fixture RPC method"),
        };
        json!({"jsonrpc":"2.0", "id":request["id"], "result":value})
    }
}
impl FakeRpc {
    /// Starts a local JSON-RPC fixture with a default three-block chain.
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let shared = Arc::clone(&state);
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        let (stream, _) = result.unwrap();
                        connections.spawn(serve(stream, Arc::clone(&shared)));
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        let rpc = Self {
            state,
            endpoint,
            server: Arc::new(Server(task.abort_handle())),
        };
        rpc.set_chain(vec![block(0, 10, 0), block(1, 11, 10), block(2, 12, 11)]);
        rpc
    }
    /// Returns an Alloy provider connected to this fixture.
    pub fn provider(&self) -> RootProvider {
        ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(self.endpoint.parse().unwrap())
    }
    /// Replaces the fixture chain.
    pub fn set_chain(&self, blocks: Vec<Block>) {
        self.state.lock().unwrap().set_chain(blocks);
    }
    /// Returns all JSON-RPC calls observed by the fixture.
    pub fn calls(&self) -> Vec<RpcCall> {
        self.state.lock().unwrap().calls.clone()
    }
}
/// Serves one JSON-RPC connection until its request stream ends.
async fn serve(mut stream: TcpStream, state: Arc<Mutex<State>>) {
    let mut bytes = Vec::new();
    let request: Value = loop {
        let mut buffer = [0; 4096];
        let Ok(count) = stream.read(&mut buffer).await else {
            return;
        };
        if count == 0 {
            return;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..index]).unwrap();
            let length: usize = header
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() >= index + 4 + length {
                break serde_json::from_slice(&bytes[index + 4..index + 4 + length]).unwrap();
            }
        }
    };
    let log_delay = state.lock().unwrap().log_delay;
    if request["method"] == "eth_getLogs" && !log_delay.is_zero() {
        tokio::time::sleep(log_delay).await;
    }
    let pending = state.lock().unwrap().pending;
    if pending {
        return std::future::pending().await;
    }
    let body = {
        let mut state = state.lock().unwrap();
        match request {
            Value::Array(requests) => Value::Array(
                requests
                    .into_iter()
                    .map(|request| state.response(request))
                    .collect(),
            ),
            request => state.response(request),
        }
        .to_string()
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

// Minimal metadata-only store for source/engine lifecycle tests. Entity journaling
// is covered separately by engine tests; this fixture does not model persistence.
use raven_engine::{
    BlockProcessor, BlockPtr, ChainStore, EngineError, EntityChange, EntityStore, EntityValue,
    Network,
};
use raven_evm::{BlockBatch, LiteBlockHeader, Update};
use rpc_block_crawler_datasource::ChainIdentity;

#[derive(Clone, Default)]
pub struct MetadataStore {
    state: Arc<Mutex<Metadata>>,
    changed: Arc<tokio::sync::Notify>,
}
#[derive(Default)]
struct Metadata {
    network: Option<Network<B256, ChainIdentity>>,
    headers: Vec<LiteBlockHeader>,
    operations: Vec<(bool, B256)>,
}
impl MetadataStore {
    /// Returns the latest committed block pointer.
    pub fn block_ptr(&self) -> Option<BlockPtr<B256>> {
        self.state
            .lock()
            .unwrap()
            .headers
            .last()
            .map(BlockPtr::from)
    }
    /// Returns committed and reverted block operations in order.
    pub fn operations(&self) -> Vec<(bool, B256)> {
        self.state.lock().unwrap().operations.clone()
    }
    /// Waits until an operation for `hash` is recorded.
    pub async fn wait_for(&self, hash: B256) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let notified = self.changed.notified();
                if self
                    .block_ptr()
                    .is_some_and(|block_ptr| block_ptr.hash == hash)
                {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("block_ptr was not reached");
    }
}
#[async_trait]
impl ChainStore for MetadataStore {
    type Hash = B256;
    type ChainIdentity = ChainIdentity;
    /// Returns the initialized network metadata.
    async fn network(&self) -> RavenResult<Option<Network<B256, ChainIdentity>>> {
        Ok(self.state.lock().unwrap().network.clone())
    }
    /// Stores network metadata and rejects conflicting initialization.
    async fn initialize(&self, network: &Network<B256, ChainIdentity>) -> RavenResult<()> {
        let mut state = self.state.lock().unwrap();
        if let Some(existing) = &state.network {
            if existing != network {
                return Err(EngineError::BlockPtrConflict.into());
            }
        } else {
            if !state.headers.is_empty() {
                return Err(EngineError::BlockPtrConflict.into());
            }
            state.network = Some(network.clone());
        }
        Ok(())
    }
    /// Returns the current committed block pointer.
    async fn block_ptr(&self) -> RavenResult<Option<BlockPtr<B256>>> {
        Ok(MetadataStore::block_ptr(self))
    }
    /// Finds a committed header by block number.
    async fn block_header_by_number(&self, number: u64) -> RavenResult<Option<LiteBlockHeader>> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .headers
            .iter()
            .find(|header| header.number == number)
            .cloned())
    }
    /// This fixture does not persist entities.
    async fn get_entity(
        &self,
        expected: Option<&BlockPtr<B256>>,
        _: &str,
        _: &str,
    ) -> RavenResult<Option<EntityValue>> {
        let state = self.state.lock().unwrap();
        if state.headers.last().map(BlockPtr::from).as_ref() != expected {
            return Err(EngineError::BlockPtrConflict.into());
        }
        Ok(None)
    }
    /// Records one committed block operation.
    async fn commit_block(
        &self,
        expected: Option<&BlockPtr<B256>>,
        header: &LiteBlockHeader,
        changes: Vec<EntityChange>,
    ) -> RavenResult<()> {
        let mut state = self.state.lock().unwrap();
        if state.headers.last().map(BlockPtr::from).as_ref() != expected {
            return Err(EngineError::BlockPtrConflict.into());
        }
        let adjacent = match state.headers.last() {
            Some(parent) => header.extends(parent),
            None => state
                .network
                .as_ref()
                .is_some_and(|network| network.accepts_first(header)),
        };
        if !adjacent {
            return Err(EngineError::InvalidBlock.into());
        }
        if !changes.is_empty() {
            return Err(RavenError::ChainStore(Box::new(io::Error::other(
                "metadata fixture has no entities",
            ))));
        }
        state.operations.push((true, header.hash));
        state.headers.push(header.clone());
        self.changed.notify_one();
        Ok(())
    }
    /// Records one reverted block operation.
    async fn revert_block(&self, header: &LiteBlockHeader) -> RavenResult<()> {
        let mut state = self.state.lock().unwrap();
        if state.headers.last() != Some(header) {
            return Err(EngineError::BlockPtrConflict.into());
        }
        state.headers.pop();
        state.operations.push((false, header.hash));
        self.changed.notify_one();
        Ok(())
    }
}
pub struct NoopProcessor;
#[async_trait]
impl BlockProcessor<B256, Update> for NoopProcessor {
    /// Accepts every block without changing entities.
    async fn process(&self, _: &mut dyn EntityStore, _: &BlockBatch) -> RavenResult<()> {
        Ok(())
    }
}
