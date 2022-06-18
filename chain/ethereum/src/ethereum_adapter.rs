use futures::prelude::*;
use graph::{
    blockchain::block_types::{BlockHash, ChainIdentifier},
    blockchain::IngestorError,
    components::ethereum::{EthereumBlock, LightEthereumBlock},
    prelude::{
        anyhow::anyhow,
        async_trait,
        futures03::{self, compat::Future01CompatExt, FutureExt},
        info, retry,
        slog::Logger,
        tokio::try_join,
        trace,
        web3::{
            self,
            api::Web3,
            types::{BlockId, BlockNumber as Web3BlockNumber, TransactionReceipt, H256},
        },
        BlockNumber, Error, TryFutureExt,
    },
};
use jsonrpc_core::futures::StreamExt;
use std::pin::Pin;
use std::sync::Arc;

use crate::{adapter::EthereumAdapterTrait, transport::Transport};

#[derive(Clone)]
pub struct EthereumAdapter {
    logger: Logger,
    hostname: Arc<String>,
    provider: String,
    web3: Arc<Web3<Transport>>,
}

impl EthereumAdapter {
    pub async fn new(
        logger: Logger,
        provider: String,
        hostname: String,
        transport: Transport,
    ) -> Self {
        let web3 = Arc::new(Web3::new(transport));
        Self {
            logger,
            hostname: Arc::new(hostname),
            provider,
            web3,
        }
    }
}

#[async_trait]
impl EthereumAdapterTrait for EthereumAdapter {
    fn hostname(&self) -> &str {
        &self.hostname.as_str()
    }

    fn provider(&self) -> &str {
        &self.provider.as_str()
    }

    async fn net_identifiers(&self) -> Result<ChainIdentifier, Error> {
        let web3 = self.web3.clone();
        let net_version_future = web3.net().version();
        let gen_block_hash_future = web3
            .eth()
            .block(BlockId::Number(Web3BlockNumber::Number(0.into())));

        let (net_version, genesis_block_hash) =
            try_join!(net_version_future, gen_block_hash_future).map_err(|e| {
                anyhow!(
                    "Ethereum node took too long to read network identifiers: {}",
                    e
                )
            })?;

        let genesis_block_hash = genesis_block_hash
            .map(|gen_block| gen_block.hash.map(BlockHash::from))
            .flatten()
            .ok_or_else(|| anyhow!("Ethereum node could not find genesis block"));

        let ident = ChainIdentifier {
            net_version,
            genesis_block_hash: genesis_block_hash?,
        };

        Ok(ident)
    }

    fn latest_block(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = IngestorError> + Send + Unpin> {
        let web3 = self.web3.clone();
        Box::new(
            retry("eth_getBlockByNumber(latest) with txs RPC call", logger)
                .no_limit()
                .timeout_secs(10)
                .run(move || {
                    let web3 = web3.clone();
                    async move {
                        let block_opt = web3
                            .eth()
                            .block_with_txs(Web3BlockNumber::Latest.into())
                            .await
                            .map_err(|e| {
                                anyhow!("could not get latest block from Ethereum: {}", e)
                            })?;
                        block_opt
                            .ok_or_else(|| anyhow!("no latest block returned from Ethereum").into())
                    }
                })
                .map_err(|e| {
                    e.into_inner().unwrap_or_else(|| {
                        anyhow!("Ethereum node took too long to get latest block").into()
                    })
                })
                .boxed()
                .compat(),
        )
    }

    fn latest_block_header(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = web3::types::Block<H256>, Error = IngestorError> + Send> {
        let web3 = self.web3.clone();

        Box::new(
            retry("eth_getBlockByNumber(latest) no txs RPC call", logger)
                .no_limit()
                .timeout_secs(10)
                .run(move || {
                    let web3 = web3.clone();
                    async move {
                        let block_opt = web3
                            .eth()
                            .block(Web3BlockNumber::Latest.into())
                            .await
                            .map_err(|e| {
                                anyhow!("Ethereum node took too long to read latest block: {}", e)
                            })?;
                        block_opt
                            .ok_or_else(|| anyhow!("no latest block returned from Ethereum").into())
                    }
                })
                .map_err(move |e| {
                    e.into_inner().unwrap_or_else(move || {
                        anyhow!("Ethereum node took too long to return latest block").into()
                    })
                })
                .boxed()
                .compat(),
        )
    }

    fn load_block(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = Error> + Send> {
        Box::new(
            self.block_by_hash(&logger, block_hash)
                .and_then(move |block_opt| {
                    block_opt.ok_or_else(move || {
                        anyhow!(
                            "Ethereum node could not find block with hash {}",
                            block_hash
                        )
                    })
                }),
        )
    }

    /// Find a block by its hash.
    fn block_by_hash(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send> {
        let web3 = self.web3.clone();
        let logger = logger.clone();
        Box::new(
            retry("eth_getBlockByHash RPC call", &logger)
                .no_limit()
                .timeout_secs(10)
                .run(move || {
                    Box::pin(web3.eth().block_with_txs(BlockId::Hash(block_hash.into())))
                        .compat()
                        .from_err()
                        .compat()
                })
                .map_err(move |e| {
                    e.into_inner().unwrap_or_else(move || {
                        anyhow!("Ethereum node took too long to return block {}", block_hash)
                    })
                })
                .boxed()
                .compat(),
        )
    }

    fn load_full_block(
        &self,
        logger: &Logger,
        block: LightEthereumBlock,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<EthereumBlock, IngestorError>> + Send>>
    {
        let web3 = Arc::clone(&self.web3);
        let logger = logger.clone();
        let block_hash = block.hash.expect("block is missing block hash");

        if block.transactions.is_empty() {
            trace!(logger, "Block {} contains no transactions", block_hash);
            return Box::pin(std::future::ready(Ok(EthereumBlock {
                block: Arc::new(block),
                transaction_receipts: Vec::new(),
            })));
        }

        let hashes = block
            .transactions
            .iter()
            .map(|tx| tx.hash.clone())
            .collect::<Vec<_>>();

        let hash_stream = graph::tokio_stream::iter(hashes);
        let receipt_stream = graph::tokio_stream::StreamExt::map(hash_stream, move |tx_hash| {
            println!("tx_hash: {:?}", tx_hash);
            fetch_transaction_receipt_with_retry(web3.clone(), tx_hash, block_hash, logger.clone())
        })
        .buffered(1000);
        let receipts_future = graph::tokio_stream::StreamExt::collect::<
            Result<Vec<TransactionReceipt>, IngestorError>,
        >(receipt_stream)
        .boxed();

        let block_future =
            futures03::TryFutureExt::map_ok(receipts_future, move |transaction_receipts| {
                EthereumBlock {
                    block: Arc::new(block),
                    transaction_receipts,
                }
            });
        Box::pin(block_future)
    }
}

/// Retries fetching a single transaction receipt.
async fn fetch_transaction_receipt_with_retry(
    web3: Arc<Web3<Transport>>,
    transaction_hash: H256,
    block_hash: H256,
    logger: Logger,
) -> Result<TransactionReceipt, IngestorError> {
    let logger = logger.clone();
    let operation_name = format!(
        "batch eth_getTransactionReceipt {} RPC call",
        transaction_hash
    );
    retry(operation_name, &logger)
        .limit(10)
        .no_logging()
        .timeout_secs(180)
        .run(move || web3.eth().transaction_receipt(transaction_hash).boxed())
        .await
        .map_err(|_timeout| anyhow!(block_hash).into())
        .and_then(move |some_receipt| {
            resolve_transaction_receipt(some_receipt, transaction_hash, block_hash, logger)
        })
}

fn resolve_transaction_receipt(
    transaction_receipt: Option<TransactionReceipt>,
    transaction_hash: H256,
    block_hash: H256,
    logger: Logger,
) -> Result<TransactionReceipt, IngestorError> {
    match transaction_receipt {
        // A receipt might be missing because the block was uncled, and the transaction never
        // made it back into the main chain.
        Some(receipt) => {
            // Check if the receipt has a block hash and is for the right block. Parity nodes seem
            // to return receipts with no block hash when a transaction is no longer in the main
            // chain, so treat that case the same as a receipt being absent entirely.
            if receipt.block_hash != Some(block_hash) {
                info!(
                    logger, "receipt block mismatch";
                    "receipt_block_hash" =>
                    receipt.block_hash.unwrap_or_default().to_string(),
                    "block_hash" =>
                        block_hash.to_string(),
                    "tx_hash" => transaction_hash.to_string(),
                );

                // If the receipt came from a different block, then the Ethereum node no longer
                // considers this block to be in the main chain. Nothing we can do from here except
                // give up trying to ingest this block. There is no way to get the transaction
                // receipt from this block.
                Err(IngestorError::BlockUnavailable(block_hash.clone()))
            } else {
                Ok(receipt)
            }
        }
        None => {
            // No receipt was returned.
            //
            // This can be because the Ethereum node no longer considers this block to be part of
            // the main chain, and so the transaction is no longer in the main chain. Nothing we can
            // do from here except give up trying to ingest this block.
            //
            // This could also be because the receipt is simply not available yet. For that case, we
            // should retry until it becomes available.
            Err(IngestorError::ReceiptUnavailable(
                block_hash,
                transaction_hash,
            ))
        }
    }
}
