//! Framework metrics shared by the canonical engine and RPC crawlers.
//!
//! Instrumentation uses the `metrics` facade and is a no-op without a recorder.
//! Applications install their recorder before starting an index, and give each
//! engine and its sources the same stable, unique `index` label.

use std::{
    future::IntoFuture,
    sync::Mutex,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

#[cfg(feature = "prometheus")]
pub use metrics_exporter_prometheus::BuildError as PrometheusBuildError;

/// Installs a process-wide Prometheus recorder and HTTP scrape listener.
///
/// Call once, before starting any pipelines. The listener accepts GET requests,
/// including `/metrics`, and lives on the application's Tokio runtime (or a
/// background runtime when called outside Tokio). Returns an error if the port
/// cannot bind or another global recorder is already installed.
#[cfg(feature = "prometheus")]
pub fn install_prometheus(address: std::net::SocketAddr) -> Result<(), PrometheusBuildError> {
    use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};

    PrometheusBuilder::new()
        .with_http_listener(address)
        .set_buckets_for_metric(
            Matcher::Suffix("_seconds".to_owned()),
            &[0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0],
        )?
        .set_buckets_for_metric(
            Matcher::Full("raven_reorg_depth_blocks".to_owned()),
            &[1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0],
        )?
        .install()?;
    describe();
    Ok(())
}

/// Registers metric descriptions with the application's current recorder.
pub fn describe() {
    describe_counter!(
        "raven_blocks_committed_total",
        "Successful block commits observed by this process, including replays."
    );
    describe_counter!(
        "raven_updates_committed_total",
        "Updates in successfully committed block batches."
    );
    describe_counter!(
        "raven_blocks_reverted_total",
        "Successful block rollbacks observed by this process."
    );
    describe_counter!(
        "raven_reorg_executions_total",
        "Resynchronization attempts with at least one successful rollback."
    );
    describe_histogram!(
        "raven_reorg_depth_blocks",
        "Successful rollbacks per resynchronization attempt, including partial attempts."
    );
    describe_counter!(
        "raven_producer_generations_total",
        "Datasource generations started, including the initial generation."
    );
    describe_gauge!(
        "raven_indexed_block_number",
        "Current committed block number, or -1 when no block is committed."
    );
    describe_gauge!(
        "raven_source_head_block_number",
        "Most recently observed source head number."
    );
    describe_gauge!(
        "raven_source_head_observed_timestamp_seconds",
        "Unix timestamp of the most recent successful source head observation."
    );
    describe_gauge!(
        "raven_indexing_backlog_blocks",
        "Eligible blocks still to commit, accounting for start height and confirmations."
    );
    describe_histogram!(
        "raven_block_processing_duration_seconds",
        "Block processing and entity change preparation duration."
    );
    describe_histogram!(
        "raven_block_commit_duration_seconds",
        "Atomic block commit duration."
    );
    describe_histogram!(
        "raven_resync_duration_seconds",
        "Canonical resynchronization duration, including preparation, rollback and replay."
    );
    describe_counter!(
        "raven_operation_errors_total",
        "Failed processing, commit or resynchronization calls."
    );
    describe_counter!(
        "raven_rpc_requests_total",
        "Provider calls completed or cancelled, including configured transport retries in one call."
    );
    describe_histogram!(
        "raven_rpc_request_duration_seconds",
        "Provider call duration, including configured transport retries."
    );
    describe_counter!(
        "raven_rpc_log_range_splits_total",
        "Rejected log ranges split into smaller requests."
    );
}

#[derive(Default)]
struct Progress {
    start_block: Option<u64>,
    indexed: Option<u64>,
    eligible: Option<u64>,
    head_observed: bool,
}

/// Metrics for one engine. Use a unique, stable name for each index in a process.
pub struct IndexMetrics {
    index: String,
    progress: Mutex<Progress>,
}

impl Default for IndexMetrics {
    fn default() -> Self {
        Self::new("default")
    }
}

impl IndexMetrics {
    pub fn new(index: impl Into<String>) -> Self {
        Self {
            index: index.into(),
            progress: Mutex::new(Progress::default()),
        }
    }

    /// Restores or updates the position from persisted state, without counting a commit.
    pub fn position(&self, start_block: u64, indexed: Option<u64>) {
        let mut progress = self
            .progress
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        progress.start_block = Some(start_block);
        progress.indexed = indexed;
        gauge!("raven_indexed_block_number", "index" => self.index.clone())
            .set(indexed.map_or(-1.0, |number| number as f64));
        self.backlog(&progress);
    }

    /// Observes an existing head read; `eligible` is the policy's processable height.
    pub fn head(&self, number: u64, eligible: Option<u64>) {
        gauge!("raven_source_head_block_number", "index" => self.index.clone()).set(number as f64);
        if let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) {
            gauge!("raven_source_head_observed_timestamp_seconds", "index" => self.index.clone())
                .set(now.as_secs_f64());
        }
        let mut progress = self
            .progress
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        progress.eligible = eligible;
        progress.head_observed = true;
        self.backlog(&progress);
    }

    fn backlog(&self, progress: &Progress) {
        if !progress.head_observed {
            return;
        }
        let Some(start) = progress.start_block else {
            return;
        };
        // u128 includes the final u64 height without overflowing the next position.
        let next = progress
            .indexed
            .map_or(u128::from(start), |number| u128::from(number) + 1);
        let blocks = progress
            .eligible
            .map_or(0, |number| (u128::from(number) + 1).saturating_sub(next));
        gauge!("raven_indexing_backlog_blocks", "index" => self.index.clone()).set(blocks as f64);
    }

    /// Records a successful atomic commit. Replayed blocks count again.
    pub fn committed(&self, number: u64, updates: u64) {
        counter!("raven_blocks_committed_total", "index" => self.index.clone()).increment(1);
        counter!("raven_updates_committed_total", "index" => self.index.clone()).increment(updates);
        let mut progress = self
            .progress
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        progress.indexed = Some(number);
        gauge!("raven_indexed_block_number", "index" => self.index.clone()).set(number as f64);
        self.backlog(&progress);
    }

    pub fn producer_started(&self) {
        counter!("raven_producer_generations_total", "index" => self.index.clone()).increment(1);
    }

    pub fn processing_started(&self) -> OperationTimer {
        self.timer("process", "raven_block_processing_duration_seconds")
    }

    pub fn commit_started(&self) -> OperationTimer {
        self.timer("commit", "raven_block_commit_duration_seconds")
    }

    pub fn resync_started(&self) -> OperationTimer {
        self.timer("resync", "raven_resync_duration_seconds")
    }

    fn timer(&self, operation: &'static str, metric: &'static str) -> OperationTimer {
        OperationTimer {
            index: self.index.clone(),
            operation,
            metric,
            started: Instant::now(),
            outcome: "cancelled",
        }
    }

    /// Counts actual rollbacks in this attempt, even if it later fails or is dropped.
    pub fn reorg_started(&self) -> ReorgTracker {
        ReorgTracker {
            index: self.index.clone(),
            depth: 0,
        }
    }
}

/// A timer whose unfinished drop is recorded as cancellation, never success.
pub struct OperationTimer {
    index: String,
    operation: &'static str,
    metric: &'static str,
    started: Instant,
    outcome: &'static str,
}

impl OperationTimer {
    pub fn finish(mut self, success: bool) {
        self.outcome = if success { "success" } else { "error" };
    }
}

impl Drop for OperationTimer {
    fn drop(&mut self) {
        histogram!(self.metric, "index" => self.index.clone(), "outcome" => self.outcome)
            .record(self.started.elapsed().as_secs_f64());
        if self.outcome == "error" {
            counter!(
                "raven_operation_errors_total",
                "index" => self.index.clone(),
                "operation" => self.operation
            )
            .increment(1);
        }
    }
}

/// Tracks successful rollback depth for one resynchronization attempt.
pub struct ReorgTracker {
    index: String,
    depth: u64,
}

impl ReorgTracker {
    pub fn reverted(&mut self) {
        if self.depth == 0 {
            counter!("raven_reorg_executions_total", "index" => self.index.clone()).increment(1);
        }
        self.depth += 1;
        counter!("raven_blocks_reverted_total", "index" => self.index.clone()).increment(1);
    }
}

impl Drop for ReorgTracker {
    fn drop(&mut self) {
        if self.depth > 0 {
            histogram!("raven_reorg_depth_blocks", "index" => self.index.clone())
                .record(self.depth as f64);
        }
    }
}

/// Provider-call metrics for one crawler and index. Clones keep the same labels.
#[derive(Clone)]
pub struct RpcMetrics {
    index: String,
    crawler: &'static str,
}

impl RpcMetrics {
    pub fn new(index: impl Into<String>, crawler: &'static str) -> Self {
        Self {
            index: index.into(),
            crawler,
        }
    }

    /// Observes a provider future and returns its result without altering transport behavior.
    /// Dropping this future records cancellation only after the request has been polled.
    pub async fn request<R, E>(
        &self,
        method: &'static str,
        request: impl IntoFuture<Output = Result<R, E>>,
    ) -> Result<R, E> {
        let mut timer = RpcTimer {
            metrics: self,
            method,
            started: Instant::now(),
            outcome: "cancelled",
        };
        let result = request.into_future().await;
        timer.outcome = if result.is_ok() { "success" } else { "error" };
        result
    }

    pub fn log_range_split(&self) {
        counter!(
            "raven_rpc_log_range_splits_total",
            "index" => self.index.clone(),
            "crawler" => self.crawler
        )
        .increment(1);
    }
}

struct RpcTimer<'a> {
    metrics: &'a RpcMetrics,
    method: &'static str,
    started: Instant,
    outcome: &'static str,
}

impl Drop for RpcTimer<'_> {
    fn drop(&mut self) {
        let labels = [
            ("index", self.metrics.index.clone()),
            ("crawler", self.metrics.crawler.to_owned()),
            ("method", self.method.to_owned()),
            ("outcome", self.outcome.to_owned()),
        ];
        counter!("raven_rpc_requests_total", &labels).increment(1);
        histogram!("raven_rpc_request_duration_seconds", &labels)
            .record(self.started.elapsed().as_secs_f64());
    }
}
