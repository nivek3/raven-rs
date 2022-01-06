use std::time::Duration;
use tracing::{info, instrument};

use core::blockchain::block_ingestor::BlockIngestor;

#[tokio::main]
async fn main() {
    start_block_ingestor(Duration::from_millis(1), "".to_string()).await;
}

#[instrument]
async fn start_block_ingestor(block_polling_interval: Duration, chain: String) {
    // Create Ethereum block ingestor and spawn a thread to run it.
    info!(
        "Starting block ingestor for network, network_name = {}",
        chain
    );

    let block_ingestor =
        BlockIngestor::<ethereum::Chain>::new(chain.ingestor_adapter(), block_polling_interval)
            .expect("failed to create Ethereum block ingestor");

    // Run the Ethereum block ingestor in the background
    tokio::spawn(async {
        block_ingestor.into_polling_stream();
    });
}
