//! Uniswap V3 entities and mappings implemented by the application in Rust.
mod chain;
mod config;
mod entities;
mod error;
mod events;
mod handlers;
mod intervals;
mod math;
mod parser;
mod positions;
mod source;
mod storage;
mod ticks;
use alloy_provider::ProviderBuilder;
pub use bigdecimal::BigDecimal;
pub use chain::ChainConfig;
pub use config::Config;
pub use entities::*;
pub use error::ExampleError;
pub use events::{Burn, Initialize, Mint, PoolCreated, PositionManager, Swap};
pub use handlers::Mapping;
use raven_engine::{CancellationToken, FinalityPolicy, RavenResult};
use raven_evm::{B256, Pipeline};
use raven_postgres::PostgresChainStore;
use rpc_block_crawler_datasource::ChainIdentity;
use std::sync::Arc;

/// Assembles and runs the configured Uniswap V3 indexing pipeline.
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
    let mapping = Mapping::new(
        Arc::clone(&provider),
        config.factory,
        config.position_manager,
        ChainConfig::load(config.chain_config.as_deref())?,
    );
    let parser = parser::UniswapParser::new(config.factory, config.position_manager);
    let source = source::UniswapSource::new(provider, config.position_manager)?;
    let store = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Ok(()),
        result = PostgresChainStore::<B256, ChainIdentity>::connect_with_schema_sql(
            &config.database_url,&config.schema,&config.network_name,storage::SCHEMA_SQL,storage::Storage,
        ) => result?,
    };
    let mut pipeline = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store)
        .from_block(config.start_block)
        .finality_policy(FinalityPolicy::Confirmations(config.confirmations))
        .cancellation_token(cancellation)
        .parser(parser, (mapping,))
        .build()?;
    pipeline.run().await
}
