#![allow(dead_code)]

use raven_evm::{Address, B256, Block, Log, RpcLog};
use rpc_log_crawler_datasource::{ProviderBuilder, RootProvider};
use serde_json::{Value, json};
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

/// Creates a block fixture for one parent-linked height.
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
    blocks: BTreeMap<B256, Block>,
    canonical: BTreeMap<u64, B256>,
    pub logs: BTreeMap<B256, Vec<RpcLog>>,
    calls: Vec<RpcCall>,
    pub max_log_range: Option<u64>,
    pub switch_after_range: Option<Vec<Block>>,
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
        let result = match call.method.as_str() {
            "eth_chainId" => json!("0x1"),
            "eth_getBlockByNumber" => {
                let tag = call.params[0].as_str().unwrap();
                let hash = if tag == "latest" {
                    self.canonical.last_key_value().map(|(_, hash)| hash)
                } else {
                    self.canonical.get(&parse_quantity(tag))
                };
                hash.map(|hash| json!(self.blocks[hash]))
                    .unwrap_or(Value::Null)
            }
            "eth_getBlockByHash" => {
                let hash: B256 = serde_json::from_value(call.params[0].clone()).unwrap();
                self.blocks
                    .get(&hash)
                    .map(|block| json!(block))
                    .unwrap_or(Value::Null)
            }
            "eth_getLogs" => {
                let query = &call.params[0];
                if let Some(hash) = query.get("blockHash") {
                    let hash: B256 = serde_json::from_value(hash.clone()).unwrap();
                    json!(self.logs.get(&hash).cloned().unwrap_or_default())
                } else {
                    let from = parse_quantity(query["fromBlock"].as_str().unwrap());
                    let to = parse_quantity(query["toBlock"].as_str().unwrap());
                    if self.max_log_range.is_some_and(|max| to - from + 1 > max) {
                        return json!({
                            "jsonrpc": "2.0",
                            "id": request["id"],
                            "error": {"code": -32005, "message": "block range too large"}
                        });
                    }
                    let logs: Vec<_> = (from..=to)
                        .filter_map(|number| self.canonical.get(&number))
                        .flat_map(|hash| self.logs.get(hash).cloned().unwrap_or_default())
                        .collect();
                    if let Some(blocks) = self.switch_after_range.take() {
                        self.set_chain(blocks);
                    }
                    json!(logs)
                }
            }
            _ => panic!("unexpected fixture RPC method"),
        };
        json!({"jsonrpc": "2.0", "id": request["id"], "result": result})
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

/// Parses a hexadecimal JSON-RPC quantity.
fn parse_quantity(value: &str) -> u64 {
    u64::from_str_radix(value.trim_start_matches("0x"), 16).unwrap()
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
