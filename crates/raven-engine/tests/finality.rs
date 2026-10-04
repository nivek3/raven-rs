mod fake;

use std::sync::Arc;

use raven_engine::{Engine, EngineError, FinalityPolicy, IngestOutcome, Network, RavenError};

use crate::fake::{Batch, FakeProcessor, FakeSource, FakeStore, block};

type TestEngine = Engine<FakeSource, FakeStore, FakeProcessor>;

/// Builds a linear branch whose hashes change from `fork_at` onward when requested.
fn branch(head: u64, fork_at: Option<u64>) -> Vec<Batch> {
    let mut parent = 0;
    (0..=head)
        .map(|number| {
            let hash = if fork_at.is_some_and(|fork| number >= fork) {
                200 + number
            } else {
                100 + number
            };
            let batch = block(number, hash, parent, vec![hash]);
            parent = hash;
            batch
        })
        .collect()
}

/// Creates an engine with the requested processing policy and a canonical fixture branch.
fn setup(head: u64, policy: FinalityPolicy) -> (Arc<FakeSource>, FakeStore, TestEngine) {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(branch(head, None));
    let network = Network::new(1, 1, Some(branch(0, None)[0].header.clone())).unwrap();
    let store = FakeStore::new(network);
    let engine = Engine::new(Arc::clone(&source), store.clone(), FakeProcessor::default())
        .with_finality_policy(policy);
    (source, store, engine)
}

#[test]
/// Verifies that confirmation arithmetic distinguishes genesis from no eligible block.
fn confirmation_arithmetic_distinguishes_genesis_from_no_eligible_block() {
    assert_eq!(FinalityPolicy::Head.processable_height(100), Some(100));
    assert_eq!(
        FinalityPolicy::Confirmations(12).processable_height(100),
        Some(88)
    );
    assert_eq!(
        FinalityPolicy::Confirmations(12).processable_height(12),
        Some(0)
    );
    assert_eq!(
        FinalityPolicy::Confirmations(12).processable_height(11),
        None
    );
    assert_eq!(
        FinalityPolicy::Confirmations(u64::MAX).processable_height(u64::MAX),
        Some(0)
    );
}

#[tokio::test]
/// Verifies that zero confirmations matches head execution.
async fn zero_confirmations_matches_head_execution() {
    let (_, head_store, mut head) = setup(6, FinalityPolicy::Head);
    let (_, zero_store, mut zero) = setup(6, FinalityPolicy::Confirmations(0));
    head.resync().await.unwrap();
    zero.resync().await.unwrap();
    assert_eq!(head_store.values(), zero_store.values());
    assert_eq!(head_store.operations(), zero_store.operations());
}

#[tokio::test]
/// Verifies that initialization waits until the configured start is eligible.
async fn initialization_waits_until_the_configured_start_is_eligible() {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(branch(6, None));
    let store = FakeStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), FakeProcessor::default())
        .with_finality_policy(FinalityPolicy::Confirmations(2));
    assert!(matches!(
        engine.initialize(5).await,
        Err(RavenError::Engine(EngineError::SourceBehind))
    ));
    assert!(store.network().is_none());
    source.set_chain(branch(7, None));
    engine.initialize(5).await.unwrap();
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![105]);
    assert_eq!(store.network().unwrap().start_block(), 5);
}

#[tokio::test]
/// Verifies that insufficient confirmations do not initialize genesis.
async fn insufficient_confirmations_do_not_initialize_genesis() {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(branch(0, None));
    let store = FakeStore::default();
    let mut engine = Engine::new(source, store.clone(), FakeProcessor::default())
        .with_finality_policy(FinalityPolicy::Confirmations(1));
    assert!(matches!(
        engine.initialize(0).await,
        Err(RavenError::Engine(EngineError::SourceBehind))
    ));
    assert!(store.network().is_none());
    assert!(store.operations().is_empty());
}

#[tokio::test]
/// Verifies that gap recovery applies only the eligible prefix.
async fn gap_recovery_applies_only_the_eligible_prefix() {
    let (_, store, mut engine) = setup(6, FinalityPolicy::Confirmations(2));
    engine.ingest(branch(6, None)[1].clone()).await.unwrap();
    assert_eq!(
        engine.ingest(branch(6, None)[4].clone()).await.unwrap(),
        IngestOutcome::Resync
    );
    assert_eq!(store.block_ptr().unwrap().number, 1);
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![101, 102, 103, 104]);
    assert_eq!(store.block_ptr().unwrap().number, 4);
}

#[tokio::test]
/// Verifies that immature streamed block is deferred then applied after head advances.
async fn immature_streamed_block_is_deferred_then_applied_after_head_advances() {
    let (source, store, mut engine) = setup(6, FinalityPolicy::Confirmations(2));
    engine.resync().await.unwrap();
    let immature = branch(6, None)[5].clone();
    assert_eq!(
        engine.ingest(immature.clone()).await.unwrap(),
        IngestOutcome::Deferred
    );
    assert_eq!(store.block_ptr().unwrap().number, 4);
    source.set_chain(branch(7, None));
    assert_eq!(
        engine.ingest(immature).await.unwrap(),
        IngestOutcome::Applied
    );
    assert_eq!(store.values(), vec![101, 102, 103, 104, 105]);
}

#[tokio::test]
/// Verifies that reorg replay stops at the same eligible height as clean replay.
async fn reorg_replay_stops_at_the_same_eligible_height_as_clean_replay() {
    let (source, store, mut engine) = setup(6, FinalityPolicy::Confirmations(2));
    engine.resync().await.unwrap();
    source.set_chain(branch(6, Some(3)));
    engine.resync().await.unwrap();
    let (clean_source, clean, mut replay) = setup(6, FinalityPolicy::Confirmations(2));
    clean_source.set_chain(branch(6, Some(3)));
    replay.resync().await.unwrap();
    assert_eq!(store.values(), vec![101, 102, 203, 204]);
    assert_eq!(store.values(), clean.values());
    assert_eq!(store.block_ptr(), clean.block_ptr());
}

#[tokio::test]
/// Verifies that reorg above block ptr does not revert committed blocks.
async fn reorg_above_block_ptr_does_not_revert_committed_blocks() {
    let (source, store, mut engine) = setup(6, FinalityPolicy::Confirmations(2));
    engine.resync().await.unwrap();
    let before = store.operations();
    source.set_chain(branch(6, Some(5)));
    engine.resync().await.unwrap();
    assert_eq!(store.operations(), before);
}

#[tokio::test]
/// Verifies that stricter policy on restart keeps a still canonical block ptr.
async fn stricter_policy_on_restart_keeps_a_still_canonical_block_ptr() {
    let (source, store, mut engine) = setup(6, FinalityPolicy::Head);
    engine.resync().await.unwrap();
    let before = store.operations();
    let mut restarted = Engine::new(Arc::clone(&source), store.clone(), FakeProcessor::default())
        .with_finality_policy(FinalityPolicy::Confirmations(5));
    restarted.initialize(0).await.unwrap();
    restarted.resync().await.unwrap();
    assert_eq!(store.operations(), before);
    source.set_chain(branch(7, None));
    assert_eq!(
        restarted.ingest(branch(7, None)[7].clone()).await.unwrap(),
        IngestOutcome::Deferred
    );
    assert_eq!(store.block_ptr().unwrap().number, 6);
}

#[tokio::test]
/// Verifies that head reduction does not undo a canonical block ptr above the new limit.
async fn head_reduction_does_not_undo_a_canonical_block_ptr_above_the_new_limit() {
    let (source, store, mut engine) = setup(8, FinalityPolicy::Confirmations(2));
    engine.resync().await.unwrap();
    let before = store.operations();
    source.set_chain(branch(7, None));
    engine.resync().await.unwrap();
    assert_eq!(store.block_ptr().unwrap().number, 6);
    assert_eq!(store.operations(), before);
}

#[tokio::test]
/// Verifies that orphaned block ptr is reverted even if no replacement is yet eligible.
async fn orphaned_block_ptr_is_reverted_even_if_no_replacement_is_yet_eligible() {
    let (source, store, mut engine) = setup(8, FinalityPolicy::Confirmations(2));
    engine.resync().await.unwrap();
    source.set_chain(branch(4, Some(3)));
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![101, 102]);
    assert_eq!(store.block_ptr().unwrap().number, 2);
}

#[tokio::test]
/// Verifies that no eligible height still allows reverting an orphan to the start block.
async fn no_eligible_height_still_allows_reverting_an_orphan_to_the_start_block() {
    let (source, store, mut engine) = setup(4, FinalityPolicy::Head);
    engine.resync().await.unwrap();
    let mut delayed = Engine::new(Arc::clone(&source), store.clone(), FakeProcessor::default())
        .with_finality_policy(FinalityPolicy::Confirmations(10));
    source.set_chain(branch(4, Some(1)));
    delayed.resync().await.unwrap();
    assert!(store.block_ptr().is_none());
    assert!(store.values().is_empty());
    assert_eq!(store.network().unwrap().start_block(), 1);
}

#[tokio::test]
/// Verifies that head shrinking during preflight discards the prepared replacement.
async fn head_shrinking_during_preflight_discards_the_prepared_replacement() {
    let (source, store, mut engine) = setup(6, FinalityPolicy::Confirmations(2));
    engine.resync().await.unwrap();
    let before = store.operations();
    source.set_chain(branch(6, Some(3)));
    source.switch_on_number_lookup(branch(5, Some(3)));
    assert!(matches!(
        engine.resync().await,
        Err(RavenError::Engine(EngineError::SourceChanged))
    ));
    assert_eq!(store.operations(), before);
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![101, 102, 203]);
}
