use raven_engine::RavenError;

#[derive(Debug, thiserror::Error)]
pub enum ExampleError {
    #[error("parsed event is missing log metadata")]
    MissingMetadata,
    #[error("application entity state is invalid")]
    InvalidPoolState,
    #[error("invalid application chain policy")]
    InvalidChainConfig,
    #[error("invalid RPC endpoint")]
    InvalidEndpoint,
    #[error("RPC request timed out")]
    RequestTimeout,
}

impl From<ExampleError> for RavenError {
    /// Classifies application validation failures as source errors.
    fn from(error: ExampleError) -> Self {
        Self::Handler(Box::new(error))
    }
}
