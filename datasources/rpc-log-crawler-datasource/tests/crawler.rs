mod fake;

use fake::{FakeRpc, block, hash, log};
use raven_engine::{BlockSource, CancellationToken, Datasource};
use raven_evm::{EvmFilter, Filter, Update};
use rpc_log_crawler_datasource::{RpcLogCrawler, RpcLogCrawlerConfig};
use serde_json::json;
use std::time::Duration;

/// Creates a log crawler with deterministic test configuration.
fn crawler(rpc: &FakeRpc) -> RpcLogCrawler {
    RpcLogCrawler::from_provider_with_config(
        rpc.provider(),
        RpcLogCrawlerConfig {
            poll_interval: Duration::from_millis(10),
            ..RpcLogCrawlerConfig::default()
        },
        EvmFilter {
            blocks: false,
            logs: Some(Filter::default()),
        },
    )
    .unwrap()
}

#[test]
/// Verifies the native log query includes both range endpoints.
fn native_query_uses_an_inclusive_block_range() {
    let query = Filter::new().from_block(10u64).to_block(20u64);
    assert_eq!(
        serde_json::to_value(query).unwrap(),
        json!({"fromBlock": "0xa", "toBlock": "0x14", "topics": []})
    );
}

#[tokio::test]
/// Verifies a sequential window fetches logs once and preserves empty blocks.
async fn sequential_window_fetches_logs_once_and_emits_empty_blocks() {
    let rpc = FakeRpc::new().await;
    rpc.state
        .lock()
        .unwrap()
        .logs
        .insert(hash(11), vec![log(1, 11)]);
    let source = crawler(&rpc);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let task = tokio::spawn(async move { source.consume(1, sender, cancel).await });

    let first = receiver.recv().await.unwrap();
    let second = receiver.recv().await.unwrap();
    token.cancel();
    task.await.unwrap().unwrap();

    assert_eq!(first.header.hash, hash(11));
    assert!(matches!(first.updates.as_slice(), [Update::Log(_)]));
    assert_eq!(second.header.hash, hash(12));
    assert!(second.updates.is_empty());
    let log_calls: Vec<_> = rpc
        .calls()
        .into_iter()
        .filter(|call| call.method == "eth_getLogs")
        .collect();
    assert_eq!(log_calls.len(), 1);
    assert_eq!(
        log_calls[0].params,
        json!([{"fromBlock": "0x1", "toBlock": "0x2", "topics": []}])
    );
}

#[tokio::test]
/// Verifies range and exact reads normalize a stable block identically.
async fn range_and_exact_paths_return_the_same_batch_for_a_stable_hash() {
    let rpc = FakeRpc::new().await;
    rpc.state
        .lock()
        .unwrap()
        .logs
        .insert(hash(11), vec![log(1, 11)]);
    let source = crawler(&rpc);
    let expected = source.block_by_hash(&hash(11)).await.unwrap().unwrap();
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let producer = source.clone();
    let task = tokio::spawn(async move { producer.consume(1, sender, cancel).await });

    assert_eq!(receiver.recv().await.unwrap(), expected);
    token.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
/// Verifies rejected log ranges are retried as smaller ranges.
async fn rejected_log_ranges_are_split_until_the_node_accepts_them() {
    let rpc = FakeRpc::new().await;
    rpc.state.lock().unwrap().max_log_range = Some(1);
    let source = crawler(&rpc);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let task = tokio::spawn(async move { source.consume(1, sender, cancel).await });

    assert_eq!(receiver.recv().await.unwrap().header.hash, hash(11));
    assert_eq!(receiver.recv().await.unwrap().header.hash, hash(12));
    token.cancel();
    task.await.unwrap().unwrap();

    let log_calls: Vec<_> = rpc
        .calls()
        .into_iter()
        .filter(|call| call.method == "eth_getLogs")
        .collect();
    assert_eq!(log_calls.len(), 3);
    assert_eq!(log_calls[0].params[0]["fromBlock"], "0x1");
    assert_eq!(log_calls[0].params[0]["toBlock"], "0x2");
}

#[tokio::test]
/// Verifies a changed branch discards and retries the range.
async fn branch_change_discards_the_range_and_retries() {
    let rpc = FakeRpc::new().await;
    rpc.state.lock().unwrap().switch_after_range =
        Some(vec![block(0, 10, 0), block(1, 21, 10), block(2, 22, 21)]);
    let source = crawler(&rpc);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let task = tokio::spawn(async move { source.consume(1, sender, cancel).await });

    assert_eq!(receiver.recv().await.unwrap().header.hash, hash(21));
    assert_eq!(receiver.recv().await.unwrap().header.hash, hash(22));
    token.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
/// Verifies random access retains exact hash binding for logs.
async fn random_access_keeps_exact_hash_log_binding() {
    let rpc = FakeRpc::new().await;
    let source = crawler(&rpc);
    let batch = source.block_by_hash(&hash(11)).await.unwrap().unwrap();
    assert_eq!(batch.header.hash, hash(11));
    let log_call = rpc
        .calls()
        .into_iter()
        .find(|call| call.method == "eth_getLogs")
        .unwrap();
    assert_eq!(
        log_call.params,
        json!([{"blockHash": hash(11), "topics": []}])
    );
}

#[tokio::test]
/// Verifies a bloom match with no logs is checked through the block hash.
async fn bloom_positive_empty_range_is_confirmed_by_block_hash() {
    let rpc = FakeRpc::new().await;
    let mut possible_match = block(1, 11, 10);
    possible_match.header.inner.logs_bloom = [0xff; 256].into();
    rpc.set_chain(vec![block(0, 10, 0), possible_match, block(2, 12, 11)]);
    let source = crawler(&rpc);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let task = tokio::spawn(async move { source.consume(1, sender, cancel).await });

    assert!(receiver.recv().await.unwrap().updates.is_empty());
    assert!(receiver.recv().await.unwrap().updates.is_empty());
    token.cancel();
    task.await.unwrap().unwrap();

    let log_calls: Vec<_> = rpc
        .calls()
        .into_iter()
        .filter(|call| call.method == "eth_getLogs")
        .collect();
    assert_eq!(log_calls.len(), 2);
    assert!(
        log_calls
            .iter()
            .any(|call| call.params[0]["blockHash"] == json!(hash(11)))
    );
}

#[tokio::test]
/// Verifies invalid log crawler configuration is rejected.
async fn invalid_config_is_rejected() {
    let rpc = FakeRpc::new().await;
    let config = RpcLogCrawlerConfig {
        max_block_range: 0,
        ..RpcLogCrawlerConfig::default()
    };
    assert!(
        RpcLogCrawler::from_provider_with_config(rpc.provider(), config, EvmFilter::default())
            .is_err()
    );
}
