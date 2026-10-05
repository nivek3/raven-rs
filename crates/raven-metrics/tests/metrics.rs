use std::{
    future::{Future, pending, ready},
    task::{Context, Poll, Waker},
};

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use raven_metrics::{IndexMetrics, RpcMetrics};

fn with_recorder(test: impl FnOnce(&PrometheusHandle)) {
    let recorder = PrometheusBuilder::new()
        .set_buckets(&[1.0, 2.0, 4.0])
        .unwrap()
        .build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || test(&handle));
}

fn sample(rendered: &str, name: &str, labels: &[(&str, &str)]) -> Option<f64> {
    rendered.lines().find_map(|line| {
        let (metric, value) = line.split_once(' ')?;
        let (metric_name, metric_labels) = metric.split_once('{')?;
        if metric_name != name {
            return None;
        }
        let metric_labels = metric_labels.strip_suffix('}')?;
        if !labels.iter().all(|(key, value)| {
            let expected = format!(r#"{key}="{value}""#);
            metric_labels.split(',').any(|label| label == expected)
        }) {
            return None;
        }
        Some(value.parse().expect("numeric Prometheus sample"))
    })
}

fn complete<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("expected an immediately completed request"),
    }
}

#[test]
fn progress_tracks_eligible_work_and_rollback_without_resetting_counters() {
    with_recorder(|handle| {
        let index = IndexMetrics::new("token");
        let labels = [("index", "token")];
        index.position(100, None);
        let rendered = handle.render();
        assert_eq!(
            sample(&rendered, "raven_indexed_block_number", &labels),
            Some(-1.0)
        );
        assert_eq!(
            sample(&rendered, "raven_indexing_backlog_blocks", &labels),
            None
        );
        assert_eq!(
            sample(&rendered, "raven_blocks_committed_total", &labels),
            None
        );

        index.head(110, Some(108));
        let rendered = handle.render();
        assert_eq!(
            sample(&rendered, "raven_source_head_block_number", &labels),
            Some(110.0)
        );
        assert_eq!(
            sample(&rendered, "raven_indexing_backlog_blocks", &labels),
            Some(9.0)
        );
        assert!(
            sample(
                &rendered,
                "raven_source_head_observed_timestamp_seconds",
                &labels
            )
            .unwrap()
                > 0.0
        );

        index.position(100, Some(102));
        assert_eq!(
            sample(&handle.render(), "raven_indexing_backlog_blocks", &labels),
            Some(6.0)
        );
        assert_eq!(
            sample(&handle.render(), "raven_blocks_committed_total", &labels),
            None
        );
        index.committed(103, 2);
        index.committed(104, 3);
        let rendered = handle.render();
        assert_eq!(
            sample(&rendered, "raven_indexing_backlog_blocks", &labels),
            Some(4.0)
        );
        assert_eq!(
            sample(&rendered, "raven_blocks_committed_total", &labels),
            Some(2.0)
        );

        let mut reorg = index.reorg_started();
        reorg.reverted();
        index.position(100, Some(103));
        let rendered = handle.render();
        assert_eq!(
            sample(&rendered, "raven_indexed_block_number", &labels),
            Some(103.0)
        );
        assert_eq!(
            sample(&rendered, "raven_indexing_backlog_blocks", &labels),
            Some(5.0)
        );
        assert_eq!(
            sample(&rendered, "raven_blocks_committed_total", &labels),
            Some(2.0)
        );
        assert_eq!(
            sample(&rendered, "raven_updates_committed_total", &labels),
            Some(5.0)
        );
        drop(reorg);
        index.position(100, None);
        let rendered = handle.render();
        assert_eq!(
            sample(&rendered, "raven_indexed_block_number", &labels),
            Some(-1.0)
        );
        assert_eq!(
            sample(&rendered, "raven_indexing_backlog_blocks", &labels),
            Some(9.0)
        );
        index.committed(100, 1);

        let other = IndexMetrics::new("pool");
        other.position(200, None);
        other.head(209, Some(207));
        other.committed(200, 1);
        let rendered = handle.render();
        assert_eq!(
            sample(
                &rendered,
                "raven_indexed_block_number",
                &[("index", "pool")]
            ),
            Some(200.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_indexing_backlog_blocks",
                &[("index", "pool")]
            ),
            Some(7.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_blocks_committed_total",
                &[("index", "pool")]
            ),
            Some(1.0)
        );
        assert_eq!(
            sample(&rendered, "raven_blocks_committed_total", &labels),
            Some(3.0)
        );
        assert_eq!(
            sample(&rendered, "raven_updates_committed_total", &labels),
            Some(6.0)
        );

        index.head(2, None);
        assert_eq!(
            sample(&handle.render(), "raven_indexing_backlog_blocks", &labels),
            Some(0.0)
        );
        index.position(u64::MAX, None);
        index.head(u64::MAX, Some(u64::MAX));
        assert_eq!(
            sample(&handle.render(), "raven_indexing_backlog_blocks", &labels),
            Some(1.0)
        );
        index.committed(u64::MAX, 0);
        assert_eq!(
            sample(&handle.render(), "raven_indexing_backlog_blocks", &labels),
            Some(0.0)
        );
    });
}

#[test]
fn reorg_counts_successful_rollbacks_and_records_partial_attempt_depth() {
    with_recorder(|handle| {
        let index = IndexMetrics::new("token");
        let labels = [("index", "token")];
        drop(index.reorg_started());
        let rendered = handle.render();
        assert_eq!(
            sample(&rendered, "raven_reorg_executions_total", &labels),
            None
        );
        assert_eq!(
            sample(&rendered, "raven_reorg_depth_blocks_count", &labels),
            None
        );

        let mut attempt = index.reorg_started();
        attempt.reverted();
        let rendered = handle.render();
        assert_eq!(
            sample(&rendered, "raven_reorg_executions_total", &labels),
            Some(1.0)
        );
        assert_eq!(
            sample(&rendered, "raven_blocks_reverted_total", &labels),
            Some(1.0)
        );
        assert_eq!(
            sample(&rendered, "raven_reorg_depth_blocks_count", &labels),
            None
        );
        attempt.reverted();
        drop(attempt);

        let mut partial = index.reorg_started();
        partial.reverted();
        drop(partial);
        let rendered = handle.render();
        assert_eq!(
            sample(&rendered, "raven_reorg_executions_total", &labels),
            Some(2.0)
        );
        assert_eq!(
            sample(&rendered, "raven_blocks_reverted_total", &labels),
            Some(3.0)
        );
        assert_eq!(
            sample(&rendered, "raven_reorg_depth_blocks_count", &labels),
            Some(2.0)
        );
        assert_eq!(
            sample(&rendered, "raven_reorg_depth_blocks_sum", &labels),
            Some(3.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_reorg_depth_blocks_bucket",
                &[("index", "token"), ("le", "1")]
            ),
            Some(1.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_reorg_depth_blocks_bucket",
                &[("index", "token"), ("le", "2")]
            ),
            Some(2.0)
        );
    });
}

#[test]
fn operation_timers_distinguish_success_errors_and_unfinished_cancellation() {
    with_recorder(|handle| {
        let index = IndexMetrics::new("token");
        index.processing_started().finish(true);
        index.processing_started().finish(false);
        drop(index.processing_started());
        index.commit_started().finish(false);
        drop(index.resync_started());
        let rendered = handle.render();
        for outcome in ["success", "error", "cancelled"] {
            assert_eq!(
                sample(
                    &rendered,
                    "raven_block_processing_duration_seconds_count",
                    &[("index", "token"), ("outcome", outcome)]
                ),
                Some(1.0)
            );
        }
        assert_eq!(
            sample(
                &rendered,
                "raven_operation_errors_total",
                &[("index", "token"), ("operation", "process")]
            ),
            Some(1.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_operation_errors_total",
                &[("index", "token"), ("operation", "commit")]
            ),
            Some(1.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_operation_errors_total",
                &[("index", "token"), ("operation", "resync")]
            ),
            None
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_block_commit_duration_seconds_count",
                &[("index", "token"), ("outcome", "error")]
            ),
            Some(1.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_block_commit_duration_seconds_count",
                &[("index", "token"), ("outcome", "success")]
            ),
            None
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_resync_duration_seconds_count",
                &[("index", "token"), ("outcome", "cancelled")]
            ),
            Some(1.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_resync_duration_seconds_count",
                &[("index", "token"), ("outcome", "success")]
            ),
            None
        );
    });
}

#[test]
fn rpc_requests_preserve_results_and_count_each_polled_request_once() {
    with_recorder(|handle| {
        let rpc = RpcMetrics::new("token", "block");
        let success = Box::new(7_u8);
        let original = std::ptr::from_ref(success.as_ref());
        let returned =
            complete(rpc.request("eth_getBlockByHash", ready(Ok::<_, &str>(success)))).unwrap();
        assert_eq!(std::ptr::from_ref(returned.as_ref()), original);
        assert_eq!(*returned, 7);
        assert_eq!(
            complete(rpc.request(
                "eth_getBlockByHash",
                ready(Err::<Box<u8>, _>("provider rejected"))
            )),
            Err("provider rejected")
        );

        drop(rpc.request("eth_getBlockByHash", pending::<Result<Box<u8>, &str>>()));
        let mut cancelled =
            Box::pin(rpc.request("eth_getBlockByHash", pending::<Result<Box<u8>, &str>>()));
        let mut context = Context::from_waker(Waker::noop());
        assert!(cancelled.as_mut().poll(&mut context).is_pending());
        assert!(cancelled.as_mut().poll(&mut context).is_pending());
        drop(cancelled);
        rpc.log_range_split();
        rpc.clone().log_range_split();

        let rendered = handle.render();
        for outcome in ["success", "error", "cancelled"] {
            let labels = [
                ("index", "token"),
                ("crawler", "block"),
                ("method", "eth_getBlockByHash"),
                ("outcome", outcome),
            ];
            assert_eq!(
                sample(&rendered, "raven_rpc_requests_total", &labels),
                Some(1.0)
            );
            assert_eq!(
                sample(
                    &rendered,
                    "raven_rpc_request_duration_seconds_count",
                    &labels
                ),
                Some(1.0)
            );
        }
        assert_eq!(
            sample(
                &rendered,
                "raven_rpc_log_range_splits_total",
                &[("index", "token"), ("crawler", "block")]
            ),
            Some(2.0)
        );
        assert_eq!(
            sample(
                &rendered,
                "raven_operation_errors_total",
                &[("index", "token")]
            ),
            None
        );
    });
}
