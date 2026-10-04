mod fake;

use std::sync::Arc;

use raven_engine::{BlockPtr, ChainStore, Engine, EngineError, IngestOutcome, Network, RavenError};

use crate::fake::{Batch, FakeProcessor, FakeSource, FakeStore, block};

type TestEngine = Engine<FakeSource, FakeStore, FakeProcessor>;

/// Returns the initial four-block branch used by engine tests.
fn original() -> Vec<Batch> {
    vec![
        block(0, 10, 0, vec![]),
        block(1, 11, 10, vec![11]),
        block(2, 12, 11, vec![12]),
        block(3, 13, 12, vec![13]),
    ]
}

/// Returns a branch that diverges after block one and extends to block four.
fn replacement() -> Vec<Batch> {
    vec![
        block(0, 10, 0, vec![]),
        block(1, 11, 10, vec![11]),
        block(2, 22, 11, vec![22]),
        block(3, 23, 22, vec![]),
        block(4, 24, 23, vec![24]),
    ]
}

/// Creates an engine, source, and store initialized for a branch starting at block one.
fn setup(branch: Vec<Batch>) -> (Arc<FakeSource>, FakeStore, TestEngine) {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(branch);
    let network = Network::new(1, 1, Some(block(0, 10, 0, vec![]).header)).unwrap();
    let store = FakeStore::new(network);
    let engine = Engine::new(Arc::clone(&source), store.clone(), FakeProcessor::default());
    (source, store, engine)
}

/// Asserts that a framework error contains the expected engine error variant.
fn assert_engine_error(error: RavenError, expected: EngineError) {
    assert!(matches!(error, RavenError::Engine(actual) if actual == expected));
}

#[tokio::test]
/// Verifies that append empty block and duplicates preserve exactly once execution.
async fn append_empty_block_and_duplicates_preserve_exactly_once_execution() {
    let branch = original();
    let (source, store, mut engine) = setup(branch.clone());
    for batch in &branch[1..] {
        assert_eq!(
            engine.ingest(batch.clone()).await.unwrap(),
            IngestOutcome::Applied
        );
    }
    let empty = block(4, 14, 13, vec![]);
    let mut updated = branch.clone();
    updated.push(empty.clone());
    source.set_chain(updated);
    assert_eq!(
        engine.ingest(empty.clone()).await.unwrap(),
        IngestOutcome::Applied
    );
    assert_eq!(engine.ingest(empty).await.unwrap(), IngestOutcome::Ignored);
    assert_eq!(
        engine.ingest(branch[1].clone()).await.unwrap(),
        IngestOutcome::Ignored
    );
    assert_eq!(store.values(), vec![11, 12, 13]);
    assert_eq!(store.operations().len(), 4);
    assert_eq!(store.block_ptr().unwrap().number, 4);
}

#[tokio::test]
/// Verifies that gap is prepared by hash before any missing block is applied.
async fn gap_is_prepared_by_hash_before_any_missing_block_is_applied() {
    let branch = original();
    let (source, store, mut engine) = setup(branch.clone());
    engine.ingest(branch[1].clone()).await.unwrap();
    assert_eq!(
        engine.ingest(branch[3].clone()).await.unwrap(),
        IngestOutcome::Resync
    );
    assert_eq!(store.block_ptr().unwrap().number, 1);
    let previous_reads = source.hash_reads().len();
    engine.resync().await.unwrap();
    assert_eq!(&source.hash_reads()[previous_reads..], &[13, 12, 11]);
    assert_eq!(store.values(), vec![11, 12, 13]);
}

#[tokio::test]
/// Verifies that unavailable gap block leaves the original block ptr unchanged.
async fn unavailable_gap_block_leaves_the_original_block_ptr_unchanged() {
    let branch = original();
    let (source, store, mut engine) = setup(branch.clone());
    engine.ingest(branch[1].clone()).await.unwrap();
    source.make_unavailable(12);
    assert_engine_error(
        engine.resync().await.unwrap_err(),
        EngineError::MissingBlock,
    );
    assert_eq!(store.operations(), vec![(true, 11)]);
    assert_eq!(store.values(), vec![11]);
}

#[tokio::test]
/// Verifies that reorg reverts descending then applies ascending and matches clean replay.
async fn reorg_reverts_descending_then_applies_ascending_and_matches_clean_replay() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    source.set_chain(replacement());
    engine.resync().await.unwrap();
    assert_eq!(
        store.operations(),
        vec![
            (true, 11),
            (true, 12),
            (true, 13),
            (false, 13),
            (false, 12),
            (true, 22),
            (true, 23),
            (true, 24)
        ]
    );
    let (_, clean, mut replay) = setup(replacement());
    replay.resync().await.unwrap();
    assert_eq!(store.values(), clean.values());
    assert_eq!(store.block_ptr(), clean.block_ptr());
}

#[tokio::test]
/// Verifies that same height reorg is detected without an incoming batch.
async fn same_height_reorg_is_detected_without_an_incoming_batch() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    let mut branch = original();
    branch[3] = block(3, 33, 12, vec![33]);
    source.set_chain(branch);
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![11, 12, 33]);
    assert_eq!(store.block_ptr().unwrap().hash, 33);
}

#[tokio::test]
/// Verifies that older conflicting incoming does not choose a fork.
async fn older_conflicting_incoming_does_not_choose_a_fork() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    let stale = block(2, 99, 11, vec![99]);
    assert_eq!(engine.ingest(stale).await.unwrap(), IngestOutcome::Resync);
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![11, 12, 13]);
    assert_eq!(source.hash_reads().last(), Some(&13));
}

#[tokio::test]
/// Verifies that moving branch during preflight does not revert or apply.
async fn moving_branch_during_preflight_does_not_revert_or_apply() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    let before = store.operations();
    source.set_chain(replacement());
    source.switch_on_number_lookup(original());
    assert_engine_error(
        engine.resync().await.unwrap_err(),
        EngineError::SourceChanged,
    );
    assert_eq!(store.operations(), before);
    assert_eq!(store.values(), vec![11, 12, 13]);
}

#[tokio::test]
/// Verifies that reorg before start block fails before partial rollback.
async fn reorg_before_start_block_fails_before_partial_rollback() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    let before = store.operations();
    source.set_chain(vec![block(0, 90, 0, vec![]), block(1, 91, 90, vec![91])]);
    assert_engine_error(
        engine.resync().await.unwrap_err(),
        EngineError::ReorgBeforeStartBlock,
    );
    assert_eq!(store.operations(), before);
}

#[tokio::test]
/// Verifies that invalid replacement payload is rejected before reverting.
async fn invalid_replacement_payload_is_rejected_before_reverting() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    let before = store.operations();
    source.set_chain(replacement());
    source.reject_payload(23); // The source rejects this payload while fetching it.
    assert_engine_error(
        engine.resync().await.unwrap_err(),
        EngineError::InvalidBlock,
    );
    assert_eq!(store.operations(), before);
}

#[tokio::test]
/// Verifies that processor failure discards staged values and restart replays from block ptr.
async fn processor_failure_discards_staged_values_and_restart_replays_from_block_ptr() {
    let (source, store, _) = setup(original());
    let mut engine = Engine::new(
        Arc::clone(&source),
        store.clone(),
        FakeProcessor { fail_on: Some(12) },
    );
    assert!(matches!(
        engine.resync().await,
        Err(RavenError::Processor(_))
    ));
    assert_eq!(store.values(), vec![11]);
    assert_eq!(store.block_ptr().unwrap().hash, 11);
    let mut restarted = Engine::new(source, store.clone(), FakeProcessor::default());
    restarted.resync().await.unwrap();
    assert_eq!(store.values(), vec![11, 12, 13]);
}

#[tokio::test]
/// Verifies that failed commit keeps state and block ptr at the same block.
async fn failed_commit_keeps_state_and_block_ptr_at_the_same_block() {
    let (_, store, mut engine) = setup(original());
    store.fail_commit(Some(12));
    assert!(matches!(
        engine.resync().await,
        Err(RavenError::ChainStore(_))
    ));
    assert_eq!(store.values(), vec![11]);
    assert_eq!(store.block_ptr().unwrap().hash, 11);
    store.fail_commit(None);
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![11, 12, 13]);
}

#[tokio::test]
/// Verifies that interruption between reverts resumes from the persisted prefix.
async fn interruption_between_reverts_resumes_from_the_persisted_prefix() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    source.set_chain(replacement());
    store.fail_revert(Some(12));
    assert!(matches!(
        engine.resync().await,
        Err(RavenError::ChainStore(_))
    ));
    assert_eq!(store.values(), vec![11, 12]);
    assert_eq!(store.block_ptr().unwrap().hash, 12);
    store.fail_revert(None);
    let mut restarted = Engine::new(source, store.clone(), FakeProcessor::default());
    restarted.resync().await.unwrap();
    assert_eq!(store.values(), vec![11, 22, 24]);
}

#[tokio::test]
/// Verifies that shorter matching head pauses but a proven shorter fork resyncs.
async fn shorter_matching_head_pauses_but_a_proven_shorter_fork_resyncs() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    source.set_chain(original()[..3].to_vec());
    assert_engine_error(
        engine.resync().await.unwrap_err(),
        EngineError::SourceBehind,
    );
    assert_eq!(store.block_ptr().unwrap().number, 3);
    source.set_chain(replacement()[..3].to_vec());
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![11, 22]);
    assert_eq!(store.block_ptr().unwrap().hash, 22);
}

#[tokio::test]
/// Verifies that initialization is persistent and wrong chain cannot resume it.
async fn initialization_is_persistent_and_wrong_chain_cannot_resume_it() {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(original());
    let store = FakeStore::default();
    let mut engine = Engine::new(source, store.clone(), FakeProcessor::default());
    engine.initialize(2).await.unwrap();
    engine.resync().await.unwrap();
    let wrong = Arc::new(FakeSource::new(2));
    wrong.set_chain(original());
    let mut restarted = Engine::new(wrong, store.clone(), FakeProcessor::default());
    assert_engine_error(
        restarted.initialize(0).await.unwrap_err(),
        EngineError::ChainIdentityMismatch,
    );
    store.revert_block(&original()[3].header).unwrap();
    store.revert_block(&original()[2].header).unwrap();
    assert_eq!(engine.next_block_number(900).await.unwrap(), 2);
}

#[tokio::test]
/// Verifies that two submissions from the same position cannot overwrite each other.
async fn two_submissions_from_the_same_position_cannot_overwrite_each_other() {
    let (_, store, _) = setup(original());
    let first = original()[1].header.clone();
    ChainStore::commit_block(&store, None, &first, Vec::new())
        .await
        .unwrap();
    assert_engine_error(
        ChainStore::commit_block(&store, None, &first, Vec::new())
            .await
            .unwrap_err(),
        EngineError::BlockPtrConflict,
    );
    assert!(
        ChainStore::commit_block(
            &store,
            Some(&BlockPtr {
                number: 1,
                hash: 99
            }),
            &original()[2].header,
            Vec::new(),
        )
        .await
        .is_err()
    );
    assert_eq!(store.operations(), vec![(true, 11)]);
}

#[tokio::test]
/// Verifies that missing local history is detected before the first revert.
async fn missing_local_history_is_detected_before_the_first_revert() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    let before = store.operations();
    store.forget_header(2);
    source.set_chain(replacement());
    assert_engine_error(
        engine.resync().await.unwrap_err(),
        EngineError::InvalidLocalHistory,
    );
    assert_eq!(store.operations(), before);
    assert_eq!(store.block_ptr().unwrap().hash, 13);
}

#[tokio::test]
/// Verifies that a previously reverted hash can be applied again with a fresh journal.
async fn a_previously_reverted_hash_can_be_applied_again_with_a_fresh_journal() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    source.set_chain(replacement());
    engine.resync().await.unwrap();
    source.set_chain(original());
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![11, 12, 13]);
    source.set_chain(replacement());
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![11, 22, 24]);
}

#[tokio::test]
/// Verifies that genesis start uses no synthetic parent block ptr.
async fn genesis_start_uses_no_synthetic_parent_block_ptr() {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(original());
    let store = FakeStore::default();
    let mut engine = Engine::new(source, store.clone(), FakeProcessor::default());
    engine.initialize(0).await.unwrap();
    assert_eq!(store.block_ptr(), None);
    assert!(store.network().unwrap().parent().is_none());
    engine.resync().await.unwrap();
    assert_eq!(store.operations().first(), Some(&(true, 10)));
    assert_eq!(store.values(), vec![11, 12, 13]);
}

#[tokio::test]
/// Verifies that block source failure leaves the committed state unchanged.
async fn block_source_failure_leaves_the_committed_state_unchanged() {
    let (source, store, mut engine) = setup(original());
    engine.resync().await.unwrap();
    source.set_chain(vec![]);
    let before = store.operations();
    assert!(matches!(engine.resync().await, Err(RavenError::Source(_))));
    assert_eq!(store.operations(), before);
}

struct CachedProcessor {
    store: FakeStore,
}

#[async_trait::async_trait]
impl raven_engine::BlockProcessor<u64, u64> for CachedProcessor {
    /// Exercises cached reads, staged replacements, JSON null handling, and caught read errors.
    async fn process(
        &self,
        entities: &mut dyn raven_engine::EntityStore,
        _: &Batch,
    ) -> raven_engine::RavenResult<()> {
        use serde_json::json;
        assert_eq!(self.store.begun_blocks(), 0);
        assert_eq!(entities.get("Account", "a").await?, None);
        assert_eq!(entities.get("Account", "missing").await?, None);
        entities.put("Account", "a", &json!(10)).await?;
        assert_eq!(entities.get("Account", "a").await?, Some(json!(10)));
        entities.put("Account", "a", &json!(7)).await?;
        entities.put("Account", "null", &json!(null)).await?;
        assert_eq!(entities.get("Account", "null").await?, Some(json!(null)));
        entities.delete("Account", "null").await?;
        self.store.fail_entity_read(true);
        assert_eq!(entities.get("Account", "null").await?, None);
        assert_eq!(entities.get("Account", "missing").await?, None);
        assert_eq!(entities.get("Account", "a").await?, Some(json!(7)));
        self.store.fail_entity_read(false);
        assert_eq!(self.store.begun_blocks(), 0);
        assert_eq!(self.store.entity("Account", "a"), None);
        Ok(())
    }
}

#[tokio::test]
/// Verifies that handlers use cache before transaction and final state is reversible.
async fn handlers_use_cache_before_transaction_and_final_state_is_reversible() {
    let (source, store, _) = setup(original());
    let mut engine = Engine::new(
        source,
        store.clone(),
        CachedProcessor {
            store: store.clone(),
        },
    );
    engine.initialize(1).await.unwrap();
    engine.ingest(original()[1].clone()).await.unwrap();
    assert_eq!(store.begun_blocks(), 1);
    assert_eq!(store.entity("Account", "a"), Some(serde_json::json!(7)));
    assert_eq!(store.entity("Account", "null"), None);
    assert_eq!(store.entity_block_hash("Account", "a"), Some(11));
    assert_eq!(
        store.block_ptr(),
        Some(BlockPtr::from(&original()[1].header))
    );
    store.revert_block(&original()[1].header).unwrap();
    assert_eq!(store.entity("Account", "a"), None);
    assert_eq!(store.block_ptr(), None);
}

struct CatchReadFailure;

#[async_trait::async_trait]
impl raven_engine::BlockProcessor<u64, u64> for CatchReadFailure {
    /// Stages a value, catches a failed read, and returns success to test poisoned entity state.
    async fn process(
        &self,
        entities: &mut dyn raven_engine::EntityStore,
        _: &Batch,
    ) -> raven_engine::RavenResult<()> {
        entities
            .put("Account", "pending", &serde_json::json!(1))
            .await?;
        assert!(entities.get("Account", "missing").await.is_err());
        // Catching a failed read must not make the staged changes publishable.
        Ok(())
    }
}

#[tokio::test]
/// Verifies that caught cache read failure prevents opening write transaction.
async fn caught_cache_read_failure_prevents_opening_write_transaction() {
    let (source, store, _) = setup(original());
    store.fail_entity_read(true);
    let mut engine = Engine::new(source, store.clone(), CatchReadFailure);
    engine.initialize(1).await.unwrap();
    assert_engine_error(
        engine.ingest(original()[1].clone()).await.unwrap_err(),
        EngineError::EntityStateFailed,
    );
    assert_eq!(store.begun_blocks(), 0);
    assert_eq!(store.entity("Account", "pending"), None);
    assert_eq!(store.block_ptr(), None);
}

struct AdvanceDuringProcessing {
    store: FakeStore,
}

#[async_trait::async_trait]
impl raven_engine::BlockProcessor<u64, u64> for AdvanceDuringProcessing {
    /// Stages a value and advances the fixture pointer to make the final commit stale.
    async fn process(
        &self,
        entities: &mut dyn raven_engine::EntityStore,
        batch: &Batch,
    ) -> raven_engine::RavenResult<()> {
        entities
            .put("Account", "stale", &serde_json::json!(1))
            .await?;
        // Deliberately inject another commit to exercise the final position check.
        self.store.commit_empty_block(batch.header.clone())?;
        Ok(())
    }
}

#[tokio::test]
/// Verifies that changed block pointer rejects cached modifications.
async fn changed_block_pointer_rejects_cached_modifications() {
    let (source, store, _) = setup(original());
    let mut engine = Engine::new(
        source,
        store.clone(),
        AdvanceDuringProcessing {
            store: store.clone(),
        },
    );
    engine.initialize(1).await.unwrap();
    assert_engine_error(
        engine.ingest(original()[1].clone()).await.unwrap_err(),
        EngineError::BlockPtrConflict,
    );
    assert_eq!(store.entity("Account", "stale"), None);
    assert_eq!(store.operations(), vec![(true, 11)]);
}

struct WaitingProcessor {
    started: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl raven_engine::BlockProcessor<u64, u64> for WaitingProcessor {
    /// Stages a value then waits forever so cancellation can discard pending state.
    async fn process(
        &self,
        entities: &mut dyn raven_engine::EntityStore,
        _: &Batch,
    ) -> raven_engine::RavenResult<()> {
        entities
            .put("Account", "pending", &serde_json::json!(1))
            .await?;
        self.started.notify_one();
        std::future::pending().await
    }
}

#[tokio::test]
/// Verifies that cancelled handler discards cache without opening write transaction.
async fn cancelled_handler_discards_cache_without_opening_write_transaction() {
    let (source, store, _) = setup(original());
    let started = Arc::new(tokio::sync::Notify::new());
    let mut engine = Engine::new(
        source,
        store.clone(),
        WaitingProcessor {
            started: started.clone(),
        },
    );
    engine.initialize(1).await.unwrap();
    {
        let work = engine.ingest(original()[1].clone());
        tokio::pin!(work);
        tokio::select! {
            _ = started.notified() => {}
            result = &mut work => panic!("handler completed unexpectedly: {result:?}"),
        }
        // Leaving this scope drops the suspended handler and its cache.
    }
    assert_eq!(store.begun_blocks(), 0);
    assert_eq!(store.entity("Account", "pending"), None);
    assert_eq!(store.block_ptr(), None);
}
