mod fake;

use fake::{FakeRpc, MetadataStore, NoopProcessor, block, hash};
use raven_engine::{CancellationToken, Engine, FinalityPolicy, RunOptions};
use raven_evm::EvmFilter;
use rpc_block_crawler_datasource::{RpcBlockCrawler, RpcBlockCrawlerConfig};
use std::{sync::Arc, time::Duration};

/// Returns common runtime test options.
fn options() -> RunOptions {
    RunOptions {
        start_block: 1,
        poll_interval: Duration::from_millis(10),
        channel_size: 1,
    }
}

#[tokio::test]
/// Verifies resync handles a reorganization without a height increase.
async fn polling_resyncs_same_height_reorg_without_any_new_height() {
    let rpc = FakeRpc::new().await;
    let source =
        Arc::new(RpcBlockCrawler::from_provider(rpc.provider(), EvmFilter::default()).unwrap());
    let store = MetadataStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), NoopProcessor);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let task = tokio::spawn(async move { engine.run(source, options(), cancel).await });
    store.wait_for(hash(12)).await;
    rpc.set_chain(vec![block(0, 10, 0), block(1, 21, 10), block(2, 22, 21)]);
    store.wait_for(hash(22)).await;
    token.cancel();
    task.await.unwrap().unwrap();
    assert_eq!(
        store.operations(),
        vec![
            (true, hash(11)),
            (true, hash(12)),
            (false, hash(12)),
            (false, hash(11)),
            (true, hash(21)),
            (true, hash(22)),
        ]
    );
}

#[tokio::test]
/// Verifies confirmation finality is applied during RPC catch-up.
async fn canonical_engine_applies_confirmation_policy_to_rpc_catchup() {
    let rpc = FakeRpc::new().await;
    let source =
        Arc::new(RpcBlockCrawler::from_provider(rpc.provider(), EvmFilter::default()).unwrap());
    let store = MetadataStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), NoopProcessor)
        .with_finality_policy(FinalityPolicy::Confirmations(1));
    let token = CancellationToken::new();
    let cancel = token.clone();
    let task = tokio::spawn(async move { engine.run(source, options(), cancel).await });
    store.wait_for(hash(11)).await;
    assert_eq!(store.block_ptr().unwrap().number, 1);
    rpc.set_chain(vec![
        block(0, 10, 0),
        block(1, 11, 10),
        block(2, 12, 11),
        block(3, 13, 12),
    ]);
    store.wait_for(hash(12)).await;
    token.cancel();
    task.await.unwrap().unwrap();
    assert_eq!(store.block_ptr().unwrap().number, 2);
    assert_eq!(store.operations(), vec![(true, hash(11)), (true, hash(12))]);
}

#[tokio::test]
/// Verifies a pruned source starts from configured and persisted positions.
async fn pruned_source_starts_at_configured_start_block_and_restarts_from_persisted_position() {
    let rpc = FakeRpc::new().await;
    rpc.set_chain(vec![
        block(100, 40, 39),
        block(101, 41, 40),
        block(102, 42, 41),
    ]);
    let source =
        Arc::new(RpcBlockCrawler::from_provider(rpc.provider(), EvmFilter::default()).unwrap());
    let store = MetadataStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), NoopProcessor);
    engine.initialize(101).await.unwrap();
    engine.resync().await.unwrap();
    assert_eq!(store.block_ptr().unwrap().number, 102);
    let mut restarted = Engine::new(source, store.clone(), NoopProcessor);
    restarted.initialize(999).await.unwrap();
    restarted.resync().await.unwrap();
    assert_eq!(restarted.next_block_number(999).await.unwrap(), 103);
    assert!(rpc.calls().iter().all(|call| {
        call.method != "eth_getBlockByNumber" || call.params[0] != serde_json::json!("0x0")
    }));
    assert_eq!(store.operations(), vec![(true, hash(41)), (true, hash(42))]);
}

#[tokio::test]
/// Verifies initialization does not fetch the head suffix before the first append.
async fn initialization_and_first_append_do_not_download_the_head_suffix() {
    use raven_engine::{BlockSource, IngestOutcome};
    use raven_evm::{B256, Filter};
    /// Creates a deterministic hash for the nested header fixture.
    fn h(number: u64) -> B256 {
        let mut bytes = [0; 32];
        bytes[24..].copy_from_slice(&number.to_be_bytes());
        B256::from(bytes)
    }
    let rpc = FakeRpc::new().await;
    let chain = (0..=400)
        .map(|number| {
            let mut b = block(number, 1, 0);
            b.header.hash = h(number + 1);
            b.header.inner.parent_hash = h(number);
            b
        })
        .collect();
    rpc.set_chain(chain);
    let source = Arc::new(
        RpcBlockCrawler::from_provider(
            rpc.provider(),
            EvmFilter {
                blocks: false,
                logs: Some(Filter::default()),
            },
        )
        .unwrap(),
    );
    let store = MetadataStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), NoopProcessor);
    engine.initialize(1).await.unwrap();
    let calls = rpc.calls();
    assert!(
        calls.len() <= 8,
        "initialization must have bounded calls regardless of distance to head"
    );
    assert!(!calls.iter().any(|c| c.method == "eth_getLogs"));
    assert_eq!(store.block_ptr(), None);
    let first = source.block_by_number(1).await.unwrap().unwrap();
    rpc.state.lock().unwrap().calls.clear();
    assert_eq!(engine.ingest(first).await.unwrap(), IngestOutcome::Applied);
    assert!(
        !rpc.calls()
            .iter()
            .any(|c| c.method == "eth_getLogs" || c.method == "eth_getBlockByHash")
    );
    assert!(rpc.calls().len() <= 5);
    assert_eq!(store.block_ptr().unwrap().number, 1);
}

#[tokio::test]
/// Verifies polling preserves a window while its log request is still pending.
async fn polling_preserves_a_window_whose_log_request_is_slower_than_the_poll_interval() {
    use raven_evm::Filter;
    let rpc = FakeRpc::new().await;
    rpc.state.lock().unwrap().log_delay = Duration::from_millis(100);
    let source = Arc::new(
        RpcBlockCrawler::from_provider_with_config(
            rpc.provider(),
            RpcBlockCrawlerConfig {
                batch_size: 1,
                ..RpcBlockCrawlerConfig::default()
            },
            EvmFilter {
                blocks: false,
                logs: Some(Filter::default()),
            },
        )
        .unwrap(),
    );
    let store = MetadataStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), NoopProcessor);
    let token = CancellationToken::new();
    let cancel = token.clone();
    let task = tokio::spawn(async move { engine.run(source, options(), cancel).await });
    store.wait_for(hash(11)).await;
    token.cancel();
    task.await.unwrap().unwrap();
    let log_queries = rpc
        .calls()
        .into_iter()
        .filter(|c| c.method == "eth_getLogs")
        .count();
    assert!(
        log_queries >= 1 && log_queries <= 2,
        "only the bounded source window should download logs"
    );
}
