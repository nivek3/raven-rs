use futures::prelude::*;
use graph::{
    blockchain::block_types::{BlockHash, ChainIdentifier},
    blockchain::IngestorError,
    components::ethereum::{EthereumBlock, LightEthereumBlock},
    prelude::{
        anyhow::anyhow,
        async_trait,
        futures03::{compat::Future01CompatExt, FutureExt},
        retry,
        slog::Logger,
        tokio::try_join,
        web3::{
            self,
            api::Web3,
            types::{BlockId, BlockNumber as Web3BlockNumber, H256},
        },
        Error, TryFutureExt,
    },
};
use std::{marker::Unpin, pin::Pin};

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
        _logger: &Logger,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = IngestorError> + Send + Unpin> {
        unimplemented!()
    }

    fn latest_block_header(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = web3::types::Block<H256>, Error = IngestorError> + Send> {
        let web3 = self.web3.clone();

        Box::new(
            retry("eth_getBlockByNumber(latest) failed", logger)
                .no_limit()
                .timeout_millis(10 * 1000)
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
                        block_opt.ok_or_else(|| {
                            IngestorError::Unknown(anyhow!(
                                "no latest block returned from Ethereum"
                            ))
                        })
                    }
                })
                .map_err(|e| {
                    e.into_inner().unwrap_or_else(move || {
                        IngestorError::Unknown(anyhow!(
                            // "Ethereum node took too long to read latest block: {}",
                            // e,
                            "Ethereum node took too long to read latest block",
                        ))
                    })
                })
                .boxed()
                .compat(),
        )
    }

    fn load_block(
        &self,
        _logger: &Logger,
        _block_hash: H256,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = Error> + Send> {
        unimplemented!()
    }

    /// Find a block by its hash.
    fn block_by_hash(
        &self,
        _logger: &Logger,
        _block_hash: H256,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send> {
        unimplemented!()
    }

    fn block_by_number(
        &self,
        _logger: &Logger,
        _block_number: u64,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send> {
        unimplemented!()
    }

    fn load_full_block(
        &self,
        _logger: &Logger,
        _block: LightEthereumBlock,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<EthereumBlock, IngestorError>> + Send>>
    {
        unimplemented!()
    }
}
