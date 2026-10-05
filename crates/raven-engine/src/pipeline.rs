//! Block ingestion pipeline implemented by `Engine::run`.
//!
//! Starts the datasource producer, receives block batches, and passes them to
//! the engine for ingestion. Periodic chain checks trigger resynchronization and
//! stream restarts when the canonical branch changes.
//!
//! A generation owns one datasource task and one queue. Resynchronization cancels
//! and joins that task before dropping the old queue, preventing stale branch
//! batches from leaking into the next generation.

use std::{sync::Arc, time::Duration};

use tokio::{sync::mpsc, task::JoinSet, time};

use crate::{
    BlockProcessor, BlockSource, CancellationToken, ChainStore, Datasource, Engine, EngineError,
    IngestOutcome, RavenError, RavenResult,
};

/// Settings for the indexing start block, batch channel capacity, and chain polling.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub start_block: u64,
    pub channel_capacity: usize,
    pub poll_interval: Duration,
}

impl Default for RunOptions {
    /// Uses genesis as the configured start with a buffered channel and one-second polling.
    fn default() -> Self {
        Self {
            start_block: 0,
            channel_capacity: 64,
            poll_interval: Duration::from_secs(1),
        }
    }
}

/// Classifies how the active producer generation stopped.
enum GenerationEnd {
    Cancelled,
    Resync,
    Retry,
    Deferred,
    Finished,
    Failed(RavenError),
}

impl<S, T, P> Engine<S, T, P>
where
    S: BlockSource + 'static,
    S::Hash: Clone + Eq + 'static,
    S::Update: 'static,
    T: ChainStore<Hash = S::Hash, ChainIdentity = S::ChainIdentity>,
    P: BlockProcessor<S::Hash, S::Update>,
{
    /// Verify the committed position before startup and on every polling tick, including when
    /// height does not increase. EOF is finite completion; streaming sources should
    /// remain active until cancelled or failed. No producer survives this run future.
    pub async fn run<D>(
        &mut self,
        datasource: Arc<D>,
        options: RunOptions,
        cancellation: CancellationToken,
    ) -> RavenResult<()>
    where
        D: Datasource<Hash = S::Hash, Update = S::Update> + 'static,
    {
        if options.channel_capacity == 0 || options.poll_interval.is_zero() {
            return Err(EngineError::InvalidRunOptions.into());
        }
        let mut resync_pending = false;
        loop {
            let ready = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                result = async {
                    self.initialize(options.start_block).await?;
                    if resync_pending || !self.stream_is_current().await? {
                        self.resync().await?;
                    }
                    Ok(())
                } => result,
            };
            if let Err(error) = ready {
                if is_retryable_chain_error(&error) {
                    tokio::select! {
                        _ = cancellation.cancelled() => return Ok(()),
                        _ = time::sleep(options.poll_interval) => continue,
                    }
                }
                return Err(error);
            }
            resync_pending = false;
            let next_block = self.next_block_number(options.start_block).await?;
            tracing::info!(next_block = next_block, "Starting block stream");
            let token = cancellation.child_token();
            let _cancel_on_drop_guard = token.clone().drop_guard();
            let (sender, mut receiver) = mpsc::channel(options.channel_capacity);
            // JoinSet aborts the task if the caller drops run() before cleanup.
            let mut producer = JoinSet::new();
            let source = Arc::clone(&datasource);
            let producer_token: CancellationToken = token.clone();
            producer.spawn(async move { source.consume(next_block, sender, producer_token).await });
            let mut poll = time::interval_at(
                time::Instant::now() + options.poll_interval,
                options.poll_interval,
            );
            poll.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            let end = loop {
                tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => break GenerationEnd::Cancelled,
                    _ = poll.tick() => {
                        let current = tokio::select! {
                            biased;
                            _ = cancellation.cancelled() => break GenerationEnd::Cancelled,
                            result = self.stream_is_current() => result,
                        };
                        match current {
                            Ok(true) => poll.reset(), // Preserve the producer; avoid polling starving a ready batch.
                            Ok(false) => break GenerationEnd::Resync,
                            Err(error) if is_retryable_chain_error(&error) => break GenerationEnd::Retry,
                            Err(error) => break GenerationEnd::Failed(error),
                        }
                    },
                    batch = receiver.recv() => match batch {
                        None => break GenerationEnd::Finished,
                        Some(batch) => {
                            let result = tokio::select! {
                                biased;
                                _ = cancellation.cancelled() => break GenerationEnd::Cancelled,
                                result = self.ingest(batch) => result,
                            };
                            match result {
                                Ok(IngestOutcome::Applied | IngestOutcome::Ignored) => {}
                                Ok(IngestOutcome::Resync) => break GenerationEnd::Resync,
                                Ok(IngestOutcome::Deferred) => break GenerationEnd::Deferred,
                                Err(error) if is_retryable_chain_error(&error) => break GenerationEnd::Resync,
                                Err(error) => break GenerationEnd::Failed(error),
                            }
                        }
                    },
                }
            };
            token.cancel();
            // Keep the receiver alive until cancellation completes so a conforming
            // producer can stop without a synthetic receiver-closed failure.
            let joined = match producer.join_next().await {
                Some(result) => result
                    .map_err(|error| RavenError::Source(Box::new(error)))
                    .and_then(|result| result),
                None => Err(EngineError::MissingProducer.into()),
            };
            drop(receiver); // Old queued batches cannot enter the next generation.
            let joined = match joined {
                Err(error)
                    if is_retryable_chain_error(&error)
                        && !matches!(&end, GenerationEnd::Failed(_) | GenerationEnd::Cancelled) =>
                {
                    // A source can lose a hash while assembling a batch after a reorg.
                    // Reconcile committed state before starting a fresh producer.
                    resync_pending = true;
                    tokio::select! {
                        _ = cancellation.cancelled() => return Ok(()),
                        _ = time::sleep(options.poll_interval) => continue,
                    }
                }
                result => result,
            };
            match end {
                GenerationEnd::Failed(error) => return Err(error),
                GenerationEnd::Cancelled => {
                    joined?;
                    return Ok(());
                }
                GenerationEnd::Resync => {
                    joined?;
                    resync_pending = true;
                }
                GenerationEnd::Retry => {
                    joined?;
                }
                GenerationEnd::Deferred => {
                    joined?;
                    resync_pending = true;
                    // An eager source may resend the immature block immediately.
                    // Wait before restarting; resynchronization will fetch it when eligible.
                    tokio::select! {
                        _ = cancellation.cancelled() => return Ok(()),
                        _ = time::sleep(options.poll_interval) => {}
                    }
                }
                GenerationEnd::Finished => {
                    joined?;
                    // A finite source can finish at an orphaned tip without emitting
                    // another height. Verify once more before reporting completion.
                    return tokio::select! {
                        _ = cancellation.cancelled() => Ok(()),
                        result = self.resync() => result,
                    };
                }
            }
        }
    }
}

/// Returns whether a chain observation can be retried after the source changes or catches up.
fn is_retryable_chain_error(error: &RavenError) -> bool {
    matches!(
        error,
        RavenError::Engine(
            EngineError::SourceChanged | EngineError::SourceBehind | EngineError::MissingBlock
        )
    )
}
