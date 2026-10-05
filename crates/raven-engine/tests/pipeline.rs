mod fake;

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use raven_engine::{
    CancellationToken, Datasource, Engine, EngineError, RavenError, RavenResult, RunOptions, Sender,
};

use crate::fake::{Batch, FakeProcessor, FakeSource, FakeStore, block};

/// Returns the four-block branch used by pipeline lifecycle tests.
fn original() -> Vec<Batch> {
    vec![
        block(0, 10, 0, vec![]),
        block(1, 11, 10, vec![11]),
        block(2, 12, 11, vec![12]),
        block(3, 13, 12, vec![13]),
    ]
}

struct ChangingSource {
    random: Arc<FakeSource>,
    shutdown: CancellationToken,
    starts: Mutex<Vec<u64>>,
    active: AtomicUsize,
    queue_old_branch: bool,
}

struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    /// Decrements the active producer count when a producer generation ends.
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl Datasource for ChangingSource {
    type Hash = u64;
    type Update = u64;

    /// Switches the random-access branch and optionally queues stale batches before cancellation.
    async fn consume(
        &self,
        next_block: u64,
        sender: Sender<Batch>,
        token: CancellationToken,
    ) -> RavenResult<()> {
        assert_eq!(
            self.active.fetch_add(1, Ordering::SeqCst),
            0,
            "old producer still alive"
        );
        let _active = Active(&self.active);
        let first = {
            let mut starts = self.starts.lock().unwrap();
            starts.push(next_block);
            starts.len() == 1
        };
        if !first {
            self.shutdown.cancel();
            token.cancelled().await;
            return Ok(());
        }
        let mut replacement = original();
        replacement[3] = block(3, 33, 12, vec![33]);
        self.random.set_chain(replacement);
        if self.queue_old_branch {
            // The first batch asks for resynchronization. Remaining old batches must be
            // discarded before another producer starts, even if already buffered.
            sender.send(block(5, 15, 14, vec![15])).await.unwrap();
            sender.send(block(4, 14, 13, vec![14])).await.unwrap();
            sender.send(original()[3].clone()).await.unwrap();
        }
        token.cancelled().await;
        Ok(())
    }
}

/// Runs a branch switch and asserts producer cleanup, restart height, rollback, and replacement apply.
async fn run_branch_change(queue_old_branch: bool) {
    let random = Arc::new(FakeSource::new(1));
    random.set_chain(original());
    let store = FakeStore::default();
    let stop = CancellationToken::new();
    let datasource = Arc::new(ChangingSource {
        random: Arc::clone(&random),
        shutdown: stop.clone(),
        starts: Mutex::new(Vec::new()),
        active: AtomicUsize::new(0),
        queue_old_branch,
    });
    let mut engine = Engine::new(random, store.clone(), FakeProcessor::default());
    // Start from an already committed tip; this test exercises poll/reorg lifecycle.
    engine.initialize(1).await.unwrap();
    engine.resync().await.unwrap();
    let options = RunOptions {
        start_block: 1,
        channel_capacity: 8,
        poll_interval: Duration::from_secs(1),
    };
    tokio::time::timeout(
        Duration::from_secs(10),
        engine.run(Arc::clone(&datasource), options, stop),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(*datasource.starts.lock().unwrap(), vec![4, 4]);
    assert_eq!(datasource.active.load(Ordering::SeqCst), 0);
    assert_eq!(store.values(), vec![11, 12, 33]);
    assert_eq!(
        store.operations(),
        vec![(true, 11), (true, 12), (true, 13), (false, 13), (true, 33)]
    );
}

#[tokio::test(start_paused = true)]
/// Verifies that polling detects same height reorg without a stream message.
async fn polling_detects_same_height_reorg_without_a_stream_message() {
    run_branch_change(false).await;
}

#[tokio::test(start_paused = true)]
/// Verifies that resynchronization joins old producer and discards its queue.
async fn resynchronization_joins_old_producer_and_discards_its_queue() {
    run_branch_change(true).await;
}

#[tokio::test]
/// Verifies that finite source finishes after verifying committed state.
async fn finite_source_finishes_after_verifying_committed_state() {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(original());
    let store = FakeStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), FakeProcessor::default());
    engine
        .run(
            source,
            RunOptions {
                start_block: 1,
                ..RunOptions::default()
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(store.values(), vec![11, 12, 13]);
}

struct InterruptedSource {
    random: Arc<FakeSource>,
    starts: Mutex<Vec<u64>>,
    error: EngineError,
}

#[async_trait]
impl Datasource for InterruptedSource {
    type Hash = u64;
    type Update = u64;

    async fn consume(&self, next: u64, _: Sender<Batch>, _: CancellationToken) -> RavenResult<()> {
        let first = {
            let mut starts = self.starts.lock().unwrap();
            starts.push(next);
            starts.len() == 1
        };
        if first {
            let mut branch = original();
            branch[3] = block(3, 33, 12, vec![33]);
            self.random.set_chain(branch);
            return Err(self.error.into());
        }
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn retryable_producer_errors_resynchronize_before_restarting_from_committed_progress() {
    for error in [
        EngineError::SourceChanged,
        EngineError::SourceBehind,
        EngineError::MissingBlock,
    ] {
        let random = Arc::new(FakeSource::new(1));
        random.set_chain(original());
        let store = FakeStore::default();
        let mut engine = Engine::new(Arc::clone(&random), store.clone(), FakeProcessor::default());
        engine.initialize(1).await.unwrap();
        engine.resync().await.unwrap();
        let datasource = Arc::new(InterruptedSource {
            random,
            starts: Mutex::new(Vec::new()),
            error,
        });
        tokio::time::timeout(
            Duration::from_secs(10),
            engine.run(
                Arc::clone(&datasource),
                RunOptions {
                    start_block: 1,
                    ..RunOptions::default()
                },
                CancellationToken::new(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(*datasource.starts.lock().unwrap(), vec![4, 4]);
        assert_eq!(store.values(), vec![11, 12, 33]);
        assert_eq!(
            store.operations(),
            vec![(true, 11), (true, 12), (true, 13), (false, 13), (true, 33)]
        );
    }
}

struct FailedSource;

#[async_trait]
impl Datasource for FailedSource {
    type Hash = u64;
    type Update = u64;
    /// Fails immediately to verify that producer errors reach the pipeline caller.
    async fn consume(&self, _: u64, _: Sender<Batch>, _: CancellationToken) -> RavenResult<()> {
        Err(RavenError::Source(Box::new(std::io::Error::other(
            "fixture datasource failed",
        ))))
    }
}

#[tokio::test]
/// Verifies that a producer failure is propagated instead of becoming a successful EOF.
async fn producer_failure_is_propagated_instead_of_becoming_successful_eof() {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(original());
    let store = FakeStore::default();
    let mut engine = Engine::new(source, store.clone(), FakeProcessor::default());
    assert!(matches!(
        engine
            .run(
                Arc::new(FailedSource),
                RunOptions {
                    start_block: 1,
                    ..RunOptions::default()
                },
                CancellationToken::new()
            )
            .await,
        Err(RavenError::Source(_))
    ));
    assert!(store.values().is_empty());
    assert!(store.operations().is_empty());
}

#[tokio::test]
/// Verifies that cancellation before startup does not initialize or apply.
async fn cancellation_before_startup_does_not_initialize_or_apply() {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(original());
    let store = FakeStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), FakeProcessor::default());
    let token = CancellationToken::new();
    token.cancel();
    engine
        .run(source, RunOptions::default(), token)
        .await
        .unwrap();
    assert!(store.network().is_none());
    assert!(store.operations().is_empty());
}

struct MaturingSource {
    random: Arc<FakeSource>,
    shutdown: CancellationToken,
    starts: Mutex<Vec<(u64, tokio::time::Instant)>>,
}

#[async_trait]
impl Datasource for MaturingSource {
    type Hash = u64;
    type Update = u64;

    /// First emits an immature block, then advances the source after cancellation for a retry.
    async fn consume(
        &self,
        next_block: u64,
        sender: Sender<Batch>,
        token: CancellationToken,
    ) -> RavenResult<()> {
        let first = {
            let mut starts = self.starts.lock().unwrap();
            starts.push((next_block, tokio::time::Instant::now()));
            starts.len() == 1
        };
        if first {
            sender.send(original()[3].clone()).await.unwrap();
            token.cancelled().await;
            let mut grown = original();
            grown.push(block(4, 14, 13, vec![14]));
            self.random.set_chain(grown);
        } else {
            self.shutdown.cancel();
            token.cancelled().await;
        }
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
/// Verifies that immature batches wait for polling and restart from the committed block pointer.
async fn immature_batches_wait_for_polling_and_restart_from_the_committed_block_pointer() {
    let source = Arc::new(FakeSource::new(1));
    source.set_chain(original());
    let store = FakeStore::default();
    let stop = CancellationToken::new();
    let producer = Arc::new(MaturingSource {
        random: Arc::clone(&source),
        shutdown: stop.clone(),
        starts: Mutex::new(Vec::new()),
    });
    let mut engine = Engine::new(source, store.clone(), FakeProcessor::default())
        .with_finality_policy(raven_engine::FinalityPolicy::Confirmations(1));
    engine.initialize(1).await.unwrap();
    engine.resync().await.unwrap();
    let interval = Duration::from_secs(2);
    tokio::time::timeout(
        Duration::from_secs(10),
        engine.run(
            Arc::clone(&producer),
            RunOptions {
                start_block: 1,
                channel_capacity: 2,
                poll_interval: interval,
            },
            stop,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let starts = producer.starts.lock().unwrap();
    assert_eq!(
        starts
            .iter()
            .map(|(next_block, _)| *next_block)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert!(starts[1].1.duration_since(starts[0].1) >= interval);
    assert_eq!(store.values(), vec![11, 12, 13]);
    assert_eq!(store.block_ptr().unwrap().number, 3);
}
