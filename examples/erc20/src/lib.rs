//! ERC-20 entities derived from Transfer events.
mod config;
mod entities;
mod error;
mod events;
mod handlers;
mod storage;

pub use config::Config;
pub use entities::ERC20Balance;
pub use error::ExampleError;
pub use events::Transfer;
pub use handlers::TransferHandler;
pub use storage::balances_at;

use std::{sync::Arc, time::Duration};

use alloy_provider::ProviderBuilder;
use raven_engine::{CancellationToken, FinalityPolicy, RavenResult};
use raven_evm::{B256, LogParser, Parser, Pipeline};
use raven_postgres::PostgresChainStore;
use rpc_log_crawler_datasource::{ChainIdentity, RpcLogCrawler, RpcLogCrawlerConfig};

/// Builds and runs the ERC-20 indexing pipeline until cancellation or failure.
pub async fn run(config: Config, cancellation: CancellationToken) -> RavenResult<()> {
    let provider = Arc::new(
        ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(
                config
                    .rpc_url
                    .parse()
                    .map_err(|_| ExampleError::InvalidEndpoint)?,
            ),
    );
    let source_config = RpcLogCrawlerConfig {
        max_block_range: config.max_block_range.get(),
        block_concurrency: config.block_concurrency.get(),
        ..RpcLogCrawlerConfig::default()
    };
    let parser = LogParser::<Transfer>::new(config.token);
    let source = RpcLogCrawler::from_provider_with_config(
        Arc::clone(&provider),
        source_config,
        parser.filter(),
    )?;
    let handler = TransferHandler::new(provider, Duration::from_secs(30));
    let store = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Ok(()),
        result = PostgresChainStore::<B256, ChainIdentity>::connect_with_schema_sql(
            &config.database_url, &config.schema, &config.network_name, storage::SCHEMA_SQL, storage::Storage,
        ) => result?,
    };
    let mut pipeline = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store)
        .from_block(config.start_block)
        .finality_policy(FinalityPolicy::Confirmations(config.confirmations))
        .cancellation_token(cancellation)
        .parser(parser, (handler,))
        .build()?;
    pipeline.run().await
}
