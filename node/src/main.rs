use config::Config;
use ethereum::{EthereumAdapter, EthereumAdapterTrait, EthereumNetworks, Transport};
use futures::future::join_all;
use graph::{
    blockchain::{block_ingestor::BlockIngestor, block_types::ChainIdentifier, Blockchain},
    components::store::BlockStore,
    log::{factory::LoggerFactory, logger},
    prelude::{
        anyhow::Error,
        http::HeaderMap,
        slog::{error, info, o, Logger},
        tokio, ChainStore,
    },
};
use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, iter::FromIterator};
use structopt::StructOpt;

mod config;
mod opt;
mod store_builder;

use graph_chain_ethereum::{self as ethereum};
use graph_store_postgres::Store;
use store_builder::StoreBuilder;

#[tokio::main]
async fn main() {
    env_logger::init();

    let opt = opt::Opt::from_args();

    // Set up logger
    let logger = logger();
    // Create a component and subgraph logger factory
    let logger_factory = LoggerFactory::new(logger.clone());

    info!(logger, "Starting up");

    let config = match Config::load(&logger, &opt.clone().into()) {
        Err(e) => {
            eprintln!("configuration error: {}", e);
            std::process::exit(1);
        }
        Ok(config) => config,
    };

    // let network_name = "mainnet".to_string();
    let block_polling_interval = Duration::from_millis(10);
    println!("config : {:?}", config);
    let store_builder = StoreBuilder::new(&logger, &config).await;
    let primary_pool = store_builder.primary_pool();

    let ethereum_networks = create_ethereum_networks(logger.clone())
        .await
        .expect("Failed to parse Ethereum networks");

    let (eth_networks, ethereum_identifiers) = connect_networks(&logger, ethereum_networks).await;

    // let mut network_names = ethereum_networks.networks.keys().collect::<Vec<&String>>();
    let network_store = store_builder.network_store(ethereum_identifiers);
    let mut blockchain_map: HashMap<String, Arc<ethereum::Chain>> = HashMap::new();
    let ethereum_chains = ethereum_networks_as_chains(
        &mut blockchain_map,
        &logger,
        &eth_networks,
        &logger_factory,
        network_store.as_ref(),
    );

    start_block_ingestor(&logger, block_polling_interval, ethereum_chains).await;

    futures::future::pending::<()>().await;
}

async fn start_block_ingestor(
    logger: &Logger,
    block_polling_interval: Duration,
    chains: HashMap<String, Arc<ethereum::Chain>>,
) {
    // Create Ethereum block ingestor and spawn a thread to run it.
    info!(
        logger,
        "Starting block ingestor with {} chains [{}]",
        chains.len(),
        chains
            .keys()
            .map(|v| v.clone())
            .collect::<Vec<String>>()
            .join(", ")
    );

    // Create Ethereum block ingestor and spawn a thread to run each
    chains.iter().for_each(|(network_name, chain)| {
        info!(
            logger,
            "Starting block ingestor for network";
            "network_name" => &network_name
        );

        let block_ingestor =
            BlockIngestor::<ethereum::Chain>::new(chain.ingestor_adapter(), block_polling_interval)
                .expect("failed to create Ethereum block ingestor");

        // Run the Ethereum block ingestor in the background
        tokio::spawn(async move { block_ingestor.into_polling_stream().await });
    });
}

async fn create_ethereum_networks(logger: Logger) -> Result<EthereumNetworks, Error> {
    let mut networks = EthereumNetworks::new();
    let name = "mainnet".to_string();
    let hostname = "hostname".to_string();
    let provider = "https://ropsten.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161".to_string();
    let logger = logger.new(o!("provider" => provider.clone()));

    info!(
        logger,
        "Creating transport";
        "provider" => provider.clone(),
    );

    let mut header_map = HeaderMap::new();
    header_map.insert("Accept", "text/plain".parse().unwrap());
    let rpc = provider.as_str().clone();
    let transport = Transport::new_rpc(rpc, header_map);

    let adapter = EthereumAdapter::new(logger, provider, hostname, transport).await;

    networks.insert(name.to_string(), Arc::new(adapter.clone()));
    Ok(networks)
}

async fn connect_networks(
    logger: &Logger,
    mut eth_networks: EthereumNetworks,
) -> (EthereumNetworks, Vec<(String, Vec<ChainIdentifier>)>) {
    // The status of a provider that we learned from connecting to it
    #[derive(PartialEq)]
    enum Status {
        Broken {
            network: String,
            provider: String,
        },
        Version {
            network: String,
            ident: ChainIdentifier,
        },
    }

    // This has one entry for each provider, and therefore multiple entries
    // for each network
    let statuses = join_all(
        eth_networks
            .flatten()
            .into_iter()
            .map(|(network_name, eth_adapter)| (network_name, eth_adapter, logger.clone()))
            .map(|(network, eth_adapter, logger)| async move {
                let logger = logger.new(o!("provider" => eth_adapter.provider().to_string()));
                match tokio::time::timeout(
                    Duration::from_millis(10 * 1000),
                    eth_adapter.net_identifiers(),
                )
                .await
                .map_err(Error::from)
                {
                    // An `Err` means a timeout, an `Ok(Err)` means some other error (maybe a typo
                    // on the URL)
                    Ok(Err(e)) | Err(e) => {
                        error!(logger, "Connection to provider failed. Not using this provider";
                                       "error" =>  e.to_string());
                        Status::Broken {
                            network,
                            provider: eth_adapter.provider().to_string(),
                        }
                    }
                    Ok(Ok(ident)) => {
                        info!(
                            logger,
                            "Connected to Ethereum";
                            "network_version" => &ident.net_version,
                        );
                        Status::Version { network, ident }
                    }
                }
            }),
    )
    .await;

    // Group identifiers by network name
    let identifiers: HashMap<String, Vec<ChainIdentifier>> =
        statuses
            .into_iter()
            .fold(HashMap::new(), |mut networks, status| {
                match status {
                    Status::Broken { network, provider } => {
                        eth_networks.remove(&network, &provider)
                    }
                    Status::Version { network, ident } => {
                        networks.entry(network.to_string()).or_default().push(ident)
                    }
                }
                networks
            });
    let identifiers: Vec<_> = identifiers.into_iter().collect();
    (eth_networks, identifiers)
}

fn ethereum_networks_as_chains(
    blockchain_map: &mut HashMap<String, Arc<ethereum::Chain>>,
    logger: &Logger,
    eth_networks: &EthereumNetworks,
    logger_factory: &LoggerFactory,
    store: &Store,
) -> HashMap<String, Arc<ethereum::Chain>> {
    info!(logger, "Creating ethereum chains");
    let chains: Vec<_> = eth_networks
        .networks
        .iter()
        .filter_map(|(network_name, eth_adapters)| {
            store
                .block_store()
                .chain_store(network_name)
                .map(|chain_store| (network_name, eth_adapters, chain_store))
                .or_else(|| {
                    error!(
                        logger,
                        "No store configured for Ethereum chain {}; ignoring this chain",
                        network_name
                    );
                    None
                })
        })
        .map(|(network_name, eth_adapters, chain_store)| {
            let chain = ethereum::Chain::new(
                logger_factory.clone(),
                network_name.clone(),
                10,
                eth_adapters.clone(),
                chain_store.clone(),
            );
            (network_name.clone(), Arc::new(chain))
        })
        .collect();

    for (network_name, chain) in chains.iter().cloned() {
        blockchain_map.insert(network_name, chain);
    }

    HashMap::from_iter(chains)
}
