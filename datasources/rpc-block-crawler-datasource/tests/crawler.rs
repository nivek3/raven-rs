mod fake;

use fake::{FakeRpc, address, block, hash, log};
use raven_engine::{BlockSource, CancellationToken, Datasource, EngineError, RavenError};
use raven_evm::{EvmFilter, Filter, Update};
use rpc_block_crawler_datasource::{RpcBlockCrawler, RpcBlockCrawlerConfig};
use serde_json::json;
use std::time::Duration;

/// Creates a block crawler with deterministic test configuration.
fn crawler(rpc: &FakeRpc) -> RpcBlockCrawler {
    RpcBlockCrawler::from_provider_with_config(
        rpc.provider(),
        RpcBlockCrawlerConfig {
            poll_interval: Duration::from_millis(10),
            ..RpcBlockCrawlerConfig::default()
        },
        EvmFilter {
            blocks: false,
            logs: Some(Filter::default()),
        },
    )
    .unwrap()
}

#[test]
/// Verifies native filter conversion preserves positional topic semantics.
fn native_query_preserves_positions_and_omits_trailing_wildcards() {
    let query = Filter::new()
        .address(vec![address(1), address(2)])
        .event_signature(hash(1))
        .topic2(vec![hash(2), hash(3)])
        .at_block_hash(hash(10));
    assert_eq!(query.address.len(), 2);
    assert!(query.address.contains(&address(1)) && query.address.contains(&address(2)));
    assert_eq!(query.topics[2].len(), 2);
    let encoded = serde_json::to_value(query).unwrap();
    assert_eq!(encoded["blockHash"], json!(hash(10)));
    assert_eq!(encoded["topics"][0], json!(hash(1)));
    assert!(encoded["topics"][1].is_null());
    assert_eq!(
        serde_json::to_value(Filter::new().at_block_hash(hash(10))).unwrap(),
        json!({"blockHash": hash(10), "topics": []})
    );
}

#[tokio::test]
/// Verifies chain identity reads only the provider chain ID.
async fn identity_reads_only_chain_id_without_requiring_genesis() {
    let rpc = FakeRpc::new().await;
    let source = crawler(&rpc);
    rpc.set_chain(vec![fake::block(100, 11, 10), fake::block(101, 12, 11)]);
    let identity = source.chain_identity().await.unwrap();
    assert_eq!(identity.chain_id, 1);
    let calls = rpc.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "eth_chainId");
    assert_eq!(source.head().await.unwrap().hash, hash(12));
}

#[tokio::test]
/// Verifies height reads resolve the hash-bound payload and log query.
async fn number_lookup_resolves_hash_and_empty_logs_are_hash_bound() {
    let rpc = FakeRpc::new().await;
    let source = crawler(&rpc);
    let batch = source.block_by_number(1).await.unwrap().unwrap();
    assert_eq!(batch.header.hash, hash(11));
    assert!(batch.updates.is_empty());
    let calls = rpc.calls();
    assert_eq!(calls[0].method, "eth_getBlockByNumber");
    assert_eq!(
        calls
            .iter()
            .find(|call| call.method == "eth_getBlockByHash")
            .unwrap()
            .params,
        json!([hash(11), false])
    );
    assert_eq!(
        calls
            .iter()
            .find(|call| call.method == "eth_getLogs")
            .unwrap()
            .params,
        json!([{"blockHash": hash(11), "topics": []}])
    );
}

#[tokio::test]
/// Verifies orphan reads and streaming share payload normalization.
async fn orphan_hash_access_and_streaming_use_the_same_normalization() {
    let rpc = FakeRpc::new().await;
    rpc.state
        .lock()
        .unwrap()
        .logs
        .insert(hash(11), vec![log(1, 11), log(1, 11)]);
    let source = crawler(&rpc);
    let expected = source.block_by_hash(&hash(11)).await.unwrap().unwrap();
    assert_eq!(expected.updates.len(), 1);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let producer = source.clone();
    let task = tokio::spawn(async move { producer.consume(1, sender, cancel).await });
    assert_eq!(receiver.recv().await.unwrap(), expected);
    assert_eq!(receiver.recv().await.unwrap().header.hash, hash(12));
    token.cancel();
    task.await.unwrap().unwrap();
    rpc.set_chain(vec![block(0, 10, 0), block(1, 21, 10), block(2, 22, 21)]);
    assert_eq!(
        source.block_by_hash(&hash(11)).await.unwrap().unwrap(),
        expected
    );
    assert_eq!(
        source
            .block_by_number(1)
            .await
            .unwrap()
            .unwrap()
            .header
            .hash,
        hash(21)
    );
}

#[tokio::test]
/// Verifies unsupported hash log requests and mixed payloads fail explicitly.
async fn unsupported_hash_logs_errors_and_mixed_payloads_are_not_empty_success() {
    let rpc = FakeRpc::new().await;
    let source = crawler(&rpc);
    rpc.state.lock().unwrap().fail_logs = true;
    assert!(matches!(
        source.block_by_hash(&hash(11)).await,
        Err(RavenError::Source(_))
    ));
    rpc.state.lock().unwrap().fail_logs = false;
    rpc.state
        .lock()
        .unwrap()
        .logs
        .insert(hash(11), vec![log(1, 21)]);
    assert!(matches!(
        source.block_by_hash(&hash(11)).await,
        Err(RavenError::Source(_))
    ));
    rpc.state.lock().unwrap().logs.clear();
    rpc.state.lock().unwrap().substitute_hash = Some(hash(12));
    assert!(matches!(
        source.block_by_hash(&hash(11)).await,
        Err(RavenError::Engine(EngineError::InvalidBlock))
    ));
}

#[tokio::test]
/// Verifies a block-only filter does not request logs.
async fn no_log_demand_skips_get_logs_with_explicit_source_filters() {
    let rpc = FakeRpc::new().await;
    rpc.state
        .lock()
        .unwrap()
        .logs
        .insert(hash(11), vec![log(1, 11)]);
    let original = crawler(&rpc);
    let empty = RpcBlockCrawler::from_provider(rpc.provider(), EvmFilter::default()).unwrap();
    assert!(
        empty
            .block_by_hash(&hash(11))
            .await
            .unwrap()
            .unwrap()
            .updates
            .is_empty()
    );
    let calls = rpc.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "eth_getBlockByHash");
    assert!(matches!(
        original
            .block_by_hash(&hash(11))
            .await
            .unwrap()
            .unwrap()
            .updates[0],
        Update::Log(_)
    ));
}

#[tokio::test]
/// Verifies block updates request full transaction bodies.
async fn requested_block_updates_ask_for_full_transaction_bodies() {
    let rpc = FakeRpc::new().await;
    let mut empty = block(1, 11, 10);
    empty.transactions = Vec::<alloy_rpc_types_eth::Transaction>::new().into();
    rpc.state.lock().unwrap().blocks.insert(hash(11), empty);
    let source = RpcBlockCrawler::from_provider(
        rpc.provider(),
        EvmFilter {
            blocks: true,
            logs: None,
        },
    )
    .unwrap();
    let batch = source.block_by_hash(&hash(11)).await.unwrap().unwrap();
    assert!(matches!(batch.updates[0], Update::Block(_)));
    assert_eq!(rpc.calls()[0].params, json!([hash(11), true]));
}

#[tokio::test]
/// Verifies a changed canonical branch discards the prepared window.
async fn branch_change_during_preparation_discards_the_whole_window() {
    let rpc = FakeRpc::new().await;
    rpc.state.lock().unwrap().switch_after_hash = Some((
        hash(12),
        vec![block(0, 10, 0), block(1, 21, 10), block(2, 22, 21)],
    ));
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
/// Verifies polling emits each newly available block once.
async fn caught_up_source_polls_and_emits_new_blocks_without_repeating_old_ones() {
    let rpc = FakeRpc::new().await;
    let source = crawler(&rpc);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let task = tokio::spawn(async move { source.consume(3, sender, cancel).await });
    tokio::task::yield_now().await;
    assert!(receiver.try_recv().is_err());
    rpc.set_chain(vec![
        block(0, 10, 0),
        block(1, 11, 10),
        block(2, 12, 11),
        block(3, 13, 12),
    ]);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(receiver.recv().await.unwrap().header.hash, hash(13));
    token.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test]
/// Verifies cancellation interrupts requests and blocked sends.
async fn cancellation_interrupts_rpc_and_bounded_channel_waits() {
    for pending in [false, true] {
        let rpc = FakeRpc::new().await;
        rpc.state.lock().unwrap().pending = pending;
        let source = crawler(&rpc);
        let token = CancellationToken::new();
        let cancel = token.clone();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);
        let task = tokio::spawn(async move { source.consume(1, sender, cancel).await });
        tokio::task::yield_now().await;
        token.cancel();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
/// Verifies absent blocks and invalid crawler settings return errors.
async fn missing_blocks_and_invalid_config_are_explicit() {
    let rpc = FakeRpc::new().await;
    assert!(
        crawler(&rpc)
            .block_by_hash(&hash(99))
            .await
            .unwrap()
            .is_none()
    );
    let config = RpcBlockCrawlerConfig {
        batch_size: 0,
        ..RpcBlockCrawlerConfig::default()
    };
    assert!(
        RpcBlockCrawler::from_provider_with_config(rpc.provider(), config, EvmFilter::default())
            .is_err()
    );
}

#[test]
/// Verifies chain identities ignore an extra genesis_hash field when deserializing.
fn chain_identity_deserialization_ignores_legacy_genesis_field() {
    let identity: rpc_block_crawler_datasource::ChainIdentity =
        serde_json::from_value(json!({"chain_id": 1, "genesis_hash": hash(10)})).unwrap();
    assert_eq!(identity.chain_id, 1);
    assert_eq!(
        serde_json::to_value(identity).unwrap(),
        json!({"chain_id": 1})
    );
}
