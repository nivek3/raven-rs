use raven_engine::RavenError;

#[derive(Debug, thiserror::Error)]
pub enum ExampleError {
    #[error("required transaction metadata is missing")]
    MissingMetadata,
    #[error("application entity state is invalid")]
    InvalidPoolState,
    #[error("invalid application chain policy")]
    InvalidChainConfig,
    #[error("invalid RPC endpoint")]
    InvalidEndpoint,
    #[error("RPC request timed out")]
    RequestTimeout,
    #[error("Uniswap V3 calculation failed: {0}")]
    UniswapSdk(#[from] uniswap_v3_sdk::error::Error),
}

impl From<ExampleError> for RavenError {
    /// Classifies application validation failures as source errors.
    fn from(error: ExampleError) -> Self {
        Self::Handler(Box::new(error))
    }
}
