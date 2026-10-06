# raven-metrics

Engine and crawler instrumentation through the `metrics` facade. Install a
recorder before indexing; without one, nothing is exported.

The optional `prometheus` feature provides `install_prometheus(SocketAddr)`,
which installs one global recorder and HTTP listener per process. It uses the
current Tokio runtime, or a background runtime outside Tokio.

Use the same stable `index` label on engine and sources, distinct for concurrent
indexes. The default is `default`; avoid per-block values and credentials.
See [Monitoring](../../docs/monitoring.md) for setup, metric definitions and queries.
