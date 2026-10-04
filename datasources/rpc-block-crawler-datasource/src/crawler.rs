//! Hash-anchored EVM block acquisition built on Alloy providers.
//!
//! Sequential windows walk backwards from one canonical number anchor through
//! parent hashes. Exact reads bind blocks and logs to the requested block hash.

use std::{future::IntoFuture, sync::Arc, time::Duration};

use alloy_provider::{Provider, ProviderBuilder, RootProvider};
use alloy_rpc_types_eth::{BlockNumberOrTag, Header as RpcHeader};
use async_trait::async_trait;
use raven_engine::{
    BlockSource, CancellationToken, Datasource, EngineError, PositionError, RavenResult, Sender,
};
use raven_evm::{
    B256, Block, BlockBatch, EvmFilter, LiteBlockHeader, RpcLog, Update, build_block_batch,
};
use serde::{Deserialize, Serialize};

use crate::RpcError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainIdentity {
    pub chain_id: u64,
}

impl From<u64> for ChainIdentity {
    /// Wraps a chain ID as the source identity.
    fn from(chain_id: u64) -> Self {
        Self { chain_id }
    }
}

impl From<ChainIdentity> for u64 {
    /// Extracts the numeric chain ID.
    fn from(identity: ChainIdentity) -> Self {
        identity.chain_id
    }
}

/// Window sizing and polling settings for [`RpcBlockCrawler`].
#[derive(Debug, Clone, Copy)]
pub struct RpcBlockCrawlerConfig {
    /// Maximum number of consecutive blocks prepared in one canonical window.
    pub batch_size: u64,
    /// Delay before polling again when no eligible block is available.
    pub poll_interval: Duration,
}

impl Default for RpcBlockCrawlerConfig {
    /// Returns the standard bounded-window RPC settings.
    fn default() -> Self {
        Self {
            batch_size: 32,
            poll_interval: Duration::from_secs(1),
        }
    }
}

/// Acquires sequential block windows and hash-addressed block batches.
/// Clones share the Alloy provider and filter; configuration is fixed at construction.
#[derive(Clone)]
pub struct RpcBlockCrawler<T = RootProvider> {
    provider: Arc<T>,
    config: RpcBlockCrawlerConfig,
    filter: Arc<EvmFilter>,
}

impl RpcBlockCrawler<RootProvider> {
    /// Creates an HTTP crawler with the default configuration.
    pub fn new(endpoint: &str, filter: EvmFilter) -> RavenResult<Self> {
        Self::new_with_config(endpoint, RpcBlockCrawlerConfig::default(), filter)
    }

    /// Creates an HTTP crawler backed by Alloy's root provider.
    pub fn new_with_config(
        endpoint: &str,
        config: RpcBlockCrawlerConfig,
        filter: EvmFilter,
    ) -> RavenResult<Self> {
        let provider = ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(endpoint.parse().map_err(|_| RpcError::InvalidEndpoint)?);
        Self::from_provider_with_config(provider, config, filter)
    }
}

impl<T: Provider> RpcBlockCrawler<T> {
    /// Uses an existing Alloy provider with the default configuration.
    pub fn from_provider(provider: T, filter: EvmFilter) -> RavenResult<Self> {
        Self::from_provider_with_config(provider, RpcBlockCrawlerConfig::default(), filter)
    }

    /// Uses an existing Alloy provider, including custom transport middleware.
    pub fn from_provider_with_config(
        provider: T,
        config: RpcBlockCrawlerConfig,
        filter: EvmFilter,
    ) -> RavenResult<Self> {
        if config.batch_size == 0 {
            return Err(RpcError::InvalidConfig("batch_size must be greater than zero").into());
        }
        Ok(Self {
            provider: Arc::new(provider),
            config,
            filter: Arc::new(filter),
        })
    }

    /// Loads the header at `number` and verifies that its reported height matches.
    async fn header_at(&self, number: u64) -> RavenResult<Option<LiteBlockHeader>> {
        let block = self
            .provider
            .get_block_by_number(number.into())
            .await
            .map_err(RpcError::Request)?;
        match block {
            Some(block) if block.header.inner.number != number => {
                Err(EngineError::InvalidBlock.into())
            }
            block => Ok(block.map(|block| minimal_header(block.header))),
        }
    }

    /// Builds a parent-linked canonical batch window between inclusive heights.
    async fn window(&self, start: u64, end: u64) -> RavenResult<Vec<BlockBatch>> {
        // The endpoint is a canonical anchor for this bounded window. Every earlier
        // block is reached via parent_hash, never stitched from floating numbers.
        tracing::info!("Preparing RPC block window [{start}..{end}]");
        let anchor = self
            .header_at(end)
            .await?
            .ok_or(EngineError::MissingBlock)?;
        let mut hash = anchor.hash;
        let mut number = end;
        let mut batches: Vec<BlockBatch> = Vec::new();
        loop {
            let batch = self
                .block_by_hash(&hash)
                .await?
                .ok_or(EngineError::MissingBlock)?;
            if batch.header.number != number || (number == end && batch.header != anchor) {
                return Err(EngineError::InvalidBlock.into());
            }
            if let Some(child) = batches.last()
                && !child.header.extends(&batch.header)
            {
                return Err(EngineError::InvalidBlock.into());
            }
            hash = batch.header.parent_hash;
            batches.push(batch);
            if number == start {
                break;
            }
            number -= 1;
        }
        if self.header_at(end).await?.as_ref() != Some(&anchor) {
            return Err(EngineError::SourceChanged.into());
        }
        batches.reverse();
        Ok(batches)
    }

    /// Crawls the block history from the given starting block number.
    async fn crawl(&self, next_block: u64, sender: Sender<BlockBatch>) -> RavenResult<()> {
        let mut next = next_block;
        loop {
            let head = match self.head().await {
                Ok(head) => head,
                Err(raven_engine::RavenError::Engine(EngineError::SourceBehind)) => {
                    tokio::time::sleep(self.config.poll_interval).await;
                    continue;
                }
                Err(error) => return Err(error),
            };
            if next > head.number {
                tokio::time::sleep(self.config.poll_interval).await;
                continue;
            }
            let end = next
                .saturating_add(self.config.batch_size - 1)
                .min(head.number);
            let batches = match self.window(next, end).await {
                Ok(batches) => batches,
                Err(raven_engine::RavenError::Engine(
                    EngineError::SourceChanged | EngineError::MissingBlock,
                )) => {
                    tokio::time::sleep(self.config.poll_interval).await;
                    continue;
                }
                Err(error) => return Err(error),
            };
            for batch in batches {
                sender
                    .send(batch)
                    .await
                    .map_err(|_| RpcError::ReceiverClosed)?;
            }
            next = end.checked_add(1).ok_or(PositionError::HeightOverflow)?;
        }
    }
}

#[async_trait]
impl<T: Provider> BlockSource for RpcBlockCrawler<T> {
    type Hash = B256;
    type Update = Update;
    type ChainIdentity = ChainIdentity;

    /// Returns the chain ID reported by the provider.
    async fn chain_identity(&self) -> RavenResult<ChainIdentity> {
        Ok(ChainIdentity {
            chain_id: self
                .provider
                .get_chain_id()
                .await
                .map_err(RpcError::Request)?,
        })
    }

    /// Loads a header by height.
    async fn header_by_number(&self, number: u64) -> RavenResult<Option<LiteBlockHeader>> {
        self.header_at(number).await
    }

    /// Loads a header by its exact hash and rejects a mismatched response.
    async fn header_by_hash(&self, hash: &B256) -> RavenResult<Option<LiteBlockHeader>> {
        let block = self
            .provider
            .get_block_by_hash(*hash)
            .await
            .map_err(RpcError::Request)?;
        match block {
            Some(block) if block.header.hash != *hash => Err(EngineError::InvalidBlock.into()),
            block => Ok(block.map(|block| minimal_header(block.header))),
        }
    }

    /// Returns the provider's latest available header.
    async fn head(&self) -> RavenResult<LiteBlockHeader> {
        let block = self
            .provider
            .get_block_by_number(BlockNumberOrTag::Latest)
            .await
            .map_err(RpcError::Request)?;
        block
            .map(|block| minimal_header(block.header))
            .ok_or_else(|| EngineError::SourceBehind.into())
    }

    /// Resolves a height through its header before loading the hash-bound batch.
    async fn block_by_number(&self, number: u64) -> RavenResult<Option<BlockBatch>> {
        let Some(header) = self.header_at(number).await? else {
            return Ok(None);
        };
        let batch = self.block_by_hash(&header.hash).await?;
        if batch.as_ref().is_some_and(|batch| batch.header != header) {
            return Err(EngineError::InvalidBlock.into());
        }
        Ok(batch)
    }

    /// Loads one block and its requested logs bound to `hash`.
    async fn block_by_hash(&self, hash: &B256) -> RavenResult<Option<BlockBatch>> {
        let filter = self.filter.as_ref();
        let request = self.provider.get_block_by_hash(*hash);
        let request = if filter.blocks {
            request.full()
        } else {
            request
        };
        let (block, logs): (Option<Block>, Vec<RpcLog>) = if let Some(log_filter) = filter
            .logs
            .as_ref()
            .map(|filter| filter.clone().at_block_hash(*hash))
        {
            tokio::try_join!(request.into_future(), self.provider.get_logs(&log_filter))
                .map_err(RpcError::Request)?
        } else {
            (request.await.map_err(RpcError::Request)?, Vec::new())
        };
        let Some(block) = block else {
            if !logs.is_empty() {
                return Err(EngineError::InvalidBlock.into());
            }
            return Ok(None);
        };
        if block.header.hash != *hash {
            return Err(EngineError::InvalidBlock.into());
        }
        build_block_batch(block, logs, filter).map(Some)
    }
}

#[async_trait]
impl<T: Provider> Datasource for RpcBlockCrawler<T> {
    type Hash = B256;
    type Update = Update;

    /// Streams consecutive batches until cancellation or a source error.
    async fn consume(
        &self,
        next_block: u64,
        sender: Sender<BlockBatch>,
        cancellation: CancellationToken,
    ) -> RavenResult<()> {
        // One cancellation scope covers HTTP waits, polling, preparation and
        // backpressure. Dropping crawl leaves no producer or request task behind.
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Ok(()),
            result = self.crawl(next_block, sender) => result,
        }
    }
}

/// Converts an RPC header to the engine's canonical header representation.
fn minimal_header(header: RpcHeader) -> LiteBlockHeader {
    LiteBlockHeader {
        number: header.inner.number,
        hash: header.hash,
        parent_hash: header.inner.parent_hash,
    }
}
