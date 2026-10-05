# raven-metrics

Framework instrumentation for the canonical engine and RPC crawlers. It uses the
[`metrics`](https://docs.rs/metrics/0.24.6/metrics/) facade; applications choose and
install a recorder before starting their pipelines. Without a recorder, nothing
is exported.

The optional `prometheus` feature provides `install_prometheus(SocketAddr)`. It
installs a global recorder and HTTP scrape listener once per process. The HTTP
listener and histogram upkeep run on the application's Tokio runtime, or on a
background runtime when called outside Tokio.

Use the same stable `index` name on the engine and all its sources. Concurrent
indexes must use different names. Avoid per-block values and credentials in
labels. The default name is `default`.

See the [monitoring guide](../../docs/monitoring.md) for setup, metric definitions,
Prometheus configuration and queries.
