//! Application acquisition: pool topics and address-bound NFT topics share one
//! block order. Transfer is never requested with an address wildcard.
use crate::parser::{pool_filter, position_filter};
use alloy_primitives::{Address, B256};
use alloy_provider::RootProvider;
use async_trait::async_trait;
use raven_engine::{
    BlockSource, CancellationToken, Datasource, EngineError, RavenError, RavenResult, Sender,
};
use raven_evm::{BlockBatch, EvmError, EvmFilter, LiteBlockHeader, LogUpdate, Update};
use rpc_block_crawler_datasource::{ChainIdentity, RpcBlockCrawler};
use std::sync::Arc;
use tokio::task::JoinSet;

#[derive(Clone)]
pub(crate) struct UniswapSource {
    pools: RpcBlockCrawler<Arc<RootProvider>>,
    positions: RpcBlockCrawler<Arc<RootProvider>>,
}

impl UniswapSource {
    /// Builds the paired RPC crawlers for pool and position-manager logs.
    pub fn new(
        provider: Arc<RootProvider>,
        manager: Address,
        index: impl Into<String>,
    ) -> RavenResult<Self> {
        let index = index.into();
        Ok(Self {
            pools: RpcBlockCrawler::from_provider(
                Arc::clone(&provider),
                EvmFilter {
                    blocks: false,
                    logs: Some(pool_filter()),
                },
            )?
            .with_metrics(index.clone()),
            positions: RpcBlockCrawler::from_provider(
                provider,
                EvmFilter {
                    blocks: false,
                    logs: Some(position_filter(manager)),
                },
            )?
            .with_metrics(index),
        })
    }
    /// Merges two same-block log batches in canonical log order.
    fn merge(mut pool: BlockBatch, position: BlockBatch) -> RavenResult<BlockBatch> {
        if pool.header != position.header {
            return Err(EngineError::InvalidBlock.into());
        }
        pool.updates.extend(position.updates);
        pool.updates.sort_by_key(|update| match update {
            Update::Log(log) => log.log_index,
            Update::Block(_) => 0,
        });
        let mut previous: Option<&LogUpdate> = None;
        for log in pool.updates.iter().filter_map(|update| match update {
            Update::Log(log) => Some(log),
            Update::Block(_) => None,
        }) {
            if let Some(previous) = previous
                && (previous.log_index == log.log_index
                    || previous.transaction_index > log.transaction_index)
            {
                return Err(EvmError::LogPosition.into());
            }
            previous = Some(log);
        }
        Ok(pool)
    }
    /// Loads position-manager logs for an already acquired pool batch.
    async fn add_positions(&self, batch: BlockBatch) -> RavenResult<BlockBatch> {
        let positions = self
            .positions
            .block_by_hash(&batch.header.hash)
            .await?
            .ok_or(EngineError::MissingBlock)?;
        Self::merge(batch, positions)
    }
}
#[async_trait]
impl BlockSource for UniswapSource {
    type Hash = B256;
    type Update = Update;
    type ChainIdentity = ChainIdentity;
    /// Returns the chain identity reported by the pool crawler.
    async fn chain_identity(&self) -> RavenResult<ChainIdentity> {
        self.pools.chain_identity().await
    }
    /// Returns the latest header known to the pool crawler.
    async fn head(&self) -> RavenResult<LiteBlockHeader> {
        self.pools.head().await
    }
    /// Fetches a header by number through the pool crawler.
    async fn header_by_number(&self, number: u64) -> RavenResult<Option<LiteBlockHeader>> {
        self.pools.header_by_number(number).await
    }
    /// Fetches a header by hash through the pool crawler.
    async fn header_by_hash(&self, hash: &B256) -> RavenResult<Option<LiteBlockHeader>> {
        self.pools.header_by_hash(hash).await
    }
    /// Fetches and merges both log sets for a block number.
    async fn block_by_number(&self, number: u64) -> RavenResult<Option<BlockBatch>> {
        match self.pools.block_by_number(number).await? {
            Some(batch) => self.add_positions(batch).await.map(Some),
            None => Ok(None),
        }
    }
    /// Fetches and merges both log sets for one exact block hash.
    async fn block_by_hash(&self, hash: &B256) -> RavenResult<Option<BlockBatch>> {
        let (pool, position) = tokio::try_join!(
            self.pools.block_by_hash(hash),
            self.positions.block_by_hash(hash)
        )?;
        match (pool, position) {
            (Some(pool), Some(position)) => Self::merge(pool, position).map(Some),
            (None, None) => Ok(None),
            _ => Err(EngineError::MissingBlock.into()),
        }
    }
}
#[async_trait]
impl Datasource for UniswapSource {
    type Hash = B256;
    type Update = Update;
    /// Streams pool batches and augments each one with position-manager logs.
    async fn consume(
        &self,
        next: u64,
        sender: Sender<BlockBatch>,
        cancellation: CancellationToken,
    ) -> RavenResult<()> {
        let child = cancellation.child_token();
        // Dropping the outer producer task also cancels this child scope, so no
        // nested acquisition work can outlive the source future.
        let _cancel_on_drop_guard = child.clone().drop_guard();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let pools = self.pools.clone();
        let producer_cancel = child.clone();
        // JoinSet aborts this nested task if consume itself is dropped by the
        // engine's outer producer generation.
        let mut producer = JoinSet::new();
        producer.spawn(async move { pools.consume(next, tx, producer_cancel).await });
        let forward = async {
            while let Some(batch) = rx.recv().await {
                let batch = self.add_positions(batch).await?;
                sender
                    .send(batch)
                    .await
                    .map_err(|_| rpc_block_crawler_datasource::RpcError::ReceiverClosed)?;
            }
            Ok::<_, RavenError>(())
        };
        let result = tokio::select! {
            biased;
            _=cancellation.cancelled()=>Ok(()),
            result=forward=>result,
        };
        child.cancel();
        let produced = producer
            .join_next()
            .await
            .ok_or(EngineError::MissingProducer)?
            .map_err(|error| RavenError::Source(Box::new(error)))?;
        result?;
        produced
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, Bytes, Log};
    use alloy_provider::ProviderBuilder;
    use serde_json::{Value, json};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::oneshot,
        time::{Duration, timeout},
    };

    /// Builds a minimal header with a predictable hash for merge tests.
    fn header(hash: u8) -> LiteBlockHeader {
        LiteBlockHeader {
            number: 1,
            hash: B256::from([hash; 32]),
            parent_hash: B256::ZERO,
        }
    }

    /// Builds a logs-only batch from transaction and global log positions.
    fn batch(header: &LiteBlockHeader, positions: &[(u64, u64)]) -> BlockBatch {
        BlockBatch {
            header: header.clone(),
            updates: positions
                .iter()
                .map(|&(transaction_index, log_index)| {
                    Update::Log(LogUpdate {
                        log: Log::new_unchecked(Address::ZERO, Vec::new(), Bytes::new()),
                        block_number: header.number,
                        block_hash: header.hash,
                        transaction_hash: B256::from([transaction_index as u8; 32]),
                        transaction_index,
                        log_index,
                    })
                })
                .collect(),
        }
    }

    /// Asserts a merge failed because log ordering is invalid.
    fn assert_log_position(result: RavenResult<BlockBatch>) {
        match result {
            Err(RavenError::Source(error)) => {
                assert_eq!(
                    error.downcast_ref::<EvmError>(),
                    Some(&EvmError::LogPosition)
                );
            }
            _ => panic!("expected log position error"),
        }
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> Value {
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = stream.read(&mut buffer).await.unwrap();
            assert_ne!(count, 0, "fixture client closed before sending a request");
            bytes.extend_from_slice(&buffer[..count]);
            let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") else {
                continue;
            };
            let header = std::str::from_utf8(&bytes[..end]).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn dropping_consume_stops_the_nested_pool_crawler() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (first_request, first_seen) = oneshot::channel();
        let (release, release_response) = oneshot::channel();
        let fixture = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            assert_eq!(request["method"], "eth_getBlockByNumber");
            first_request.send(()).unwrap();
            release_response.await.unwrap();
            let block: raven_evm::Block = raven_evm::Block::default();
            let response = json!({
                "jsonrpc": "2.0",
                "id": request["id"],
                "result": block,
            })
            .to_string();
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            );
            let _ = stream.write_all(reply.as_bytes()).await;
            drop(stream);
            timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_ok()
        });
        let provider = Arc::new(
            ProviderBuilder::new()
                .disable_recommended_fillers()
                .connect_http(endpoint.parse().unwrap()),
        );
        let source = UniswapSource::new(provider, Address::ZERO, "test").unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);
        let cancellation = CancellationToken::new();
        let mut consume = Box::pin(source.consume(0, sender, cancellation));
        let first_result = timeout(Duration::from_secs(1), async {
            tokio::select! {
                _ = first_seen => Ok(()),
                result = &mut consume => Err(result),
            }
        })
        .await;
        match first_result {
            Ok(Ok(())) => {}
            Ok(Err(result)) => {
                fixture.abort();
                let _ = fixture.await;
                panic!("consume ended before the fixture released its first response: {result:?}");
            }
            Err(_) => {
                fixture.abort();
                let _ = fixture.await;
                panic!("consume did not issue its initial pool RPC request");
            }
        }
        drop(consume);
        release.send(()).unwrap();
        assert!(
            !fixture.await.unwrap(),
            "dropped consume continued the pool crawler with another RPC request"
        );
    }

    #[test]
    /// Verifies merged queries are sorted by global log index.
    fn merge_orders_logs_from_both_queries_by_global_log_index() {
        let header = header(1);
        let merged = UniswapSource::merge(
            batch(&header, &[(0, 0), (1, 3)]),
            batch(&header, &[(0, 1), (0, 2)]),
        )
        .unwrap();
        let positions: Vec<_> = merged
            .updates
            .iter()
            .map(|update| match update {
                Update::Log(log) => (log.transaction_index, log.log_index),
                Update::Block(_) => panic!("logs-only source emitted a block"),
            })
            .collect();

        assert_eq!(positions, vec![(0, 0), (0, 1), (0, 2), (1, 3)]);
    }

    #[test]
    /// Verifies merge rejects mismatched headers and invalid cross-query order.
    fn merge_rejects_incompatible_headers_and_cross_query_log_positions() {
        let first = header(1);
        let second = header(2);
        assert!(matches!(
            UniswapSource::merge(batch(&first, &[]), batch(&second, &[])),
            Err(RavenError::Engine(EngineError::InvalidBlock))
        ));

        assert_log_position(UniswapSource::merge(
            batch(&first, &[(0, 1)]),
            batch(&first, &[(0, 1)]),
        ));
        assert_log_position(UniswapSource::merge(
            batch(&first, &[(1, 0)]),
            batch(&first, &[(0, 1)]),
        ));
    }
}
