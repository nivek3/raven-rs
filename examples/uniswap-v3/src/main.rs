//! Executable entry point for the Uniswap V3 example.

use clap::Parser;
use raven_engine::CancellationToken;
use raven_example_uniswap_v3::{Config, run};

#[tokio::main]
/// Starts the indexer and coordinates graceful termination signals.
async fn main() -> std::process::ExitCode {
    dotenv::dotenv().ok();
    let config = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
    if let Some(address) = config.metrics_listen_addr {
        if let Err(error) = raven_metrics::install_prometheus(address) {
            eprintln!("Could not start Prometheus metrics listener: {error}");
            return std::process::ExitCode::FAILURE;
        }
        eprintln!("Prometheus metrics available at http://{address}/metrics");
    }
    #[cfg(unix)]
    let mut terminate =
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(_) => {
                eprintln!("Could not install termination signal handler.");
                return std::process::ExitCode::FAILURE;
            }
        };
    let cancellation = CancellationToken::new();
    let run = run(config, cancellation.clone());
    tokio::pin!(run);
    eprintln!("Starting Uniswap V3 indexing; press Ctrl-C to stop.");
    let stop = async {
        #[cfg(unix)]
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
        #[cfg(not(unix))]
        tokio::signal::ctrl_c().await
    };
    let result = tokio::select! {
        result = &mut run => result,
        signal = stop => {
            cancellation.cancel();
            let result = run.await;
            if signal.is_err() {
                eprintln!("Signal listener failed.");
                return std::process::ExitCode::FAILURE;
            }
            result
        }
    };
    match result {
        Ok(()) => {
            eprintln!("Indexer stopped.");
            std::process::ExitCode::SUCCESS
        }
        // Display only: nested transport/database Debug sources may contain URLs.
        Err(error) => {
            eprintln!("Indexer failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
