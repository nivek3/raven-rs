//! Range-based EVM log acquisition with hash-bound canonical verification.
//!
//! `eth_getLogs` accelerates sequential history. Headers still form one verified
//! parent chain, while random access delegates to the exact block crawler.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use alloy_primitives::Bloom;
use alloy_provider::{Provider, ProviderBuilder, RootProvider};
use alloy_rpc_types_eth::Header as RpcHeader;
use async_trait::async_trait;
use futures_util::{StreamExt, TryStreamExt, stream};
use raven_engine::{
    BlockSource, CancellationToken, Datasource, EngineError, PositionError, RavenError,
    RavenResult, Sender,
};
use raven_evm::{
    B256, Block, BlockBatch, EvmFilter, LiteBlockHeader, RpcLog, Update, build_block_batch,
};
use rpc_block_crawler_datasource::RpcBlockCrawler;

use crate::{ChainIdentity, RpcLogError};

/// Range, concurrency and polling settings for [`RpcLogCrawler`].
#[derive(Debug, Clone, Copy)]
pub struct RpcLogCrawlerConfig {
    /// Maximum inclusive block range requested through `eth_getLogs`.
    pub max_block_range: u64,
    /// Maximum number of block payload requests in flight within a range.
    pub block_concurrency: usize,
    /// Delay before polling again when no eligible block is available.
    pub poll_interval: Duration,
}

/// Default configuration for [`RpcLogCrawler`].
impl Default for RpcLogCrawlerConfig {
    /// Returns the standard range, concurrency and polling settings.
    fn default() -> Self {
        Self {
            max_block_range: 1_000,
            block_concurrency: 10,
            poll_interval: Duration::from_secs(1),
        }
    }
}

/// Scans sequential log ranges and delegates hash-addressed reads to `RpcBlockCrawler`.
#[derive(Clone)]
pub struct RpcLogCrawler<T = RootProvider> {
    provider: Arc<T>,
    exact: RpcBlockCrawler<Arc<T>>,
    config: RpcLogCrawlerConfig,
    filter: Arc<EvmFilter>,
}

impl<T> std::fmt::Debug for RpcLogCrawler<T> {
    /// Formats configuration and filter fields without exposing the provider.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RpcLogCrawler")
            .field("config", &self.config)
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl RpcLogCrawler<RootProvider> {
    /// Creates an HTTP log crawler with the default configuration.
    pub fn new(endpoint: &str, filter: EvmFilter) -> RavenResult<Self> {
        Self::new_with_config(endpoint, RpcLogCrawlerConfig::default(), filter)
    }

    /// Creates an HTTP log crawler backed by Alloy's root provider.
    pub fn new_with_config(
        endpoint: &str,
        config: RpcLogCrawlerConfig,
        filter: EvmFilter,
    ) -> RavenResult<Self> {
        let provider = ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(endpoint.parse().map_err(|_| RpcLogError::InvalidEndpoint)?);
        Self::from_provider_with_config(provider, config, filter)
    }
}

impl<T: Provider> RpcLogCrawler<T> {
    /// Uses an existing Alloy provider with the default configuration.
    pub fn from_provider(provider: T, filter: EvmFilter) -> RavenResult<Self> {
        Self::from_provider_with_config(provider, RpcLogCrawlerConfig::default(), filter)
    }

    /// Uses an existing Alloy provider, including custom transport middleware.
    pub fn from_provider_with_config(
        provider: T,
        config: RpcLogCrawlerConfig,
        filter: EvmFilter,
    ) -> RavenResult<Self> {
        let provider = Arc::new(provider);
        let exact = RpcBlockCrawler::from_provider(Arc::clone(&provider), filter.clone())?;
        Ok(Self {
            provider,
            exact,
            config,
            filter: Arc::new(filter),
        })
    }

    /// Loads the requested block payload and verifies its height.
    async fn block_at(&self, number: u64) -> RavenResult<Block> {
        let filter = self.filter.as_ref();
        let request = self.provider.get_block_by_number(number.into());
        let request = if filter.blocks {
            request.full()
        } else {
            request
        };
        let block = request
            .await
            .map_err(RpcLogError::Request)?
            .ok_or(EngineError::MissingBlock)?;
        if block.header.inner.number != number {
            return Err(EngineError::InvalidBlock.into());
        }
        Ok(block)
    }

    /// Loads inclusive block payloads while bounding request concurrency.
    async fn blocks_in_range(&self, start: u64, end: u64) -> RavenResult<Vec<Block>> {
        stream::iter(start..=end)
            .map(|number| self.block_at(number))
            .buffered(self.config.block_concurrency)
            .try_collect()
            .await
    }

    /// Queries requested logs across an inclusive range, splitting rejected ranges.
    async fn logs_in_range(&self, start: u64, end: u64) -> RavenResult<Vec<RpcLog>> {
        let Some(filter) = self.filter.as_ref().logs.as_ref() else {
            return Ok(Vec::new());
        };

        let mut pending = vec![(start, end)];
        let mut logs = Vec::new();
        while let Some((from, to)) = pending.pop() {
            let query = filter.clone().from_block(from).to_block(to);
            match self
                .provider
                .get_logs(&query)
                .await
                .map_err(|error| RavenError::from(RpcLogError::Request(error)))
            {
                Ok(mut range_logs) => logs.append(&mut range_logs),
                Err(error) if from < to && should_split_log_range(&error) => {
                    let middle = from + (to - from) / 2;
                    tracing::warn!(
                        from_block = from,
                        to_block = to,
                        "Splitting rejected RPC log range"
                    );
                    pending.push((middle + 1, to));
                    pending.push((from, middle));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(logs)
    }

    /// Builds canonical batches for an inclusive block range and its matching logs.
    async fn window(&self, start: u64, end: u64) -> RavenResult<Vec<BlockBatch>> {
        let filter = self.filter.as_ref();
        tracing::info!(
            from_block = start,
            to_block = end,
            "Preparing RPC log range"
        );

        let anchor = self
            .exact
            .header_by_number(end)
            .await?
            .ok_or(EngineError::MissingBlock)?;
        let (blocks, logs) = tokio::try_join!(
            self.blocks_in_range(start, end),
            self.logs_in_range(start, end),
        )?;

        let mut headers = Vec::with_capacity(blocks.len());
        let mut canonical_hashes = BTreeMap::new();
        let mut previous: Option<LiteBlockHeader> = None;
        for (offset, block) in blocks.iter().enumerate() {
            let number = start
                .checked_add(u64::try_from(offset).map_err(|_| PositionError::HeightOverflow)?)
                .ok_or(PositionError::HeightOverflow)?;
            let header = minimal_header(block.header.clone());
            if header.number != number {
                return Err(EngineError::InvalidBlock.into());
            }
            if previous
                .as_ref()
                .is_some_and(|parent| !header.extends(parent))
            {
                return Err(EngineError::SourceChanged.into());
            }
            canonical_hashes.insert(number, header.hash);
            previous = Some(header.clone());
            headers.push(header);
        }
        if previous.as_ref() != Some(&anchor) {
            return Err(EngineError::SourceChanged.into());
        }

        let mut logs_by_number: BTreeMap<u64, Vec<RpcLog>> = BTreeMap::new();
        for log in logs {
            let number = log.block_number.ok_or(EngineError::InvalidBlock)?;
            let hash = log.block_hash.ok_or(EngineError::InvalidBlock)?;
            match canonical_hashes.get(&number) {
                Some(canonical) if *canonical == hash => {}
                Some(_) => return Err(EngineError::SourceChanged.into()),
                None => return Err(EngineError::InvalidBlock.into()),
            }
            logs_by_number.entry(number).or_default().push(log);
        }

        let mut batches = Vec::with_capacity(blocks.len());
        let range_filter = filter
            .logs
            .as_ref()
            .map(|filter| filter.clone().from_block(start).to_block(end));
        for (header, block) in headers.into_iter().zip(blocks) {
            let number = header.number;
            let logs = logs_by_number.remove(&number).unwrap_or_default();
            let needs_empty_hash_binding = logs.is_empty()
                && range_filter
                    .as_ref()
                    .is_some_and(|query| bloom_may_match(query, block.header.inner.logs_bloom));
            let batch = if needs_empty_hash_binding {
                let batch = self
                    .exact
                    .block_by_hash(&header.hash)
                    .await?
                    .ok_or(EngineError::MissingBlock)?;
                if batch.header != header {
                    return Err(EngineError::InvalidBlock.into());
                }
                batch
            } else {
                build_block_batch(block, logs, filter)?
            };
            batches.push(batch);
        }
        if !logs_by_number.is_empty() {
            return Err(EngineError::InvalidBlock.into());
        }
        if self.exact.header_by_number(end).await?.as_ref() != Some(&anchor) {
            return Err(EngineError::SourceChanged.into());
        }
        Ok(batches)
    }

    /// Polls and streams consecutive canonical log-derived batch windows.
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
                .saturating_add(self.config.max_block_range - 1)
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
                    .map_err(|_| RpcLogError::ReceiverClosed)?;
            }
            next = end.checked_add(1).ok_or(PositionError::HeightOverflow)?;
        }
    }
}

#[async_trait]
impl<T: Provider> BlockSource for RpcLogCrawler<T> {
    type Hash = B256;
    type Update = Update;
    type ChainIdentity = ChainIdentity;

    /// Returns the chain identity from the exact block source.
    async fn chain_identity(&self) -> RavenResult<ChainIdentity> {
        self.exact.chain_identity().await
    }

    /// Returns the latest header from the exact block source.
    async fn head(&self) -> RavenResult<LiteBlockHeader> {
        self.exact.head().await
    }

    /// Delegates header lookup by height to the exact block source.
    async fn header_by_number(&self, number: u64) -> RavenResult<Option<LiteBlockHeader>> {
        self.exact.header_by_number(number).await
    }

    /// Delegates header lookup by hash to the exact block source.
    async fn header_by_hash(&self, hash: &B256) -> RavenResult<Option<LiteBlockHeader>> {
        self.exact.header_by_hash(hash).await
    }

    /// Loads a range-optimized batch for a single height.
    async fn block_by_number(&self, number: u64) -> RavenResult<Option<BlockBatch>> {
        self.exact.block_by_number(number).await
    }

    /// Delegates exact hash-bound batch lookup to the block crawler.
    async fn block_by_hash(&self, hash: &B256) -> RavenResult<Option<BlockBatch>> {
        self.exact.block_by_hash(hash).await
    }
}

#[async_trait]
impl<T: Provider> Datasource for RpcLogCrawler<T> {
    type Hash = B256;
    type Update = Update;

    /// Streams consecutive log-derived batches until cancellation or a source error.
    async fn consume(
        &self,
        next_block: u64,
        sender: Sender<BlockBatch>,
        cancellation: CancellationToken,
    ) -> RavenResult<()> {
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

/// Returns whether an RPC failure is likely caused by an oversized log range.
fn should_split_log_range(error: &RavenError) -> bool {
    const RANGE_LIMIT_MARKERS: &[&str] = &[
        "-32005",
        "-32000",
        "503 Service Unavailable",
        "Try with this block range",
        "block range too large",
    ];

    let RavenError::Source(source) = error else {
        return false;
    };
    let Some(error) = source.downcast_ref::<RpcLogError>() else {
        return false;
    };
    match error {
        RpcLogError::Request(error) => {
            let message = error.to_string();
            RANGE_LIMIT_MARKERS
                .iter()
                .any(|marker| message.contains(marker))
        }
        _ => false,
    }
}

/// Returns whether a header bloom can contain logs selected by `filter`.
fn bloom_may_match(filter: &alloy_rpc_types_eth::Filter, bloom: Bloom) -> bool {
    let wildcard = filter.address.is_empty() && filter.topics.iter().all(|topic| topic.is_empty());
    if wildcard {
        bloom != Bloom::default()
    } else {
        filter.matches_bloom(bloom)
    }
}
