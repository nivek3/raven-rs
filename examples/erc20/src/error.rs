use raven_engine::RavenError;

#[derive(Debug, thiserror::Error)]
pub enum ExampleError {
    #[error("stored {0} entity is invalid")]
    InvalidEntityState(&'static str),
    #[error("RPC endpoint is invalid")]
    InvalidEndpoint,
    #[error("RPC request timed out")]
    RequestTimeout,
}

impl From<ExampleError> for RavenError {
    /// Converts example-specific failures into handler failures.
    fn from(error: ExampleError) -> Self {
        Self::Handler(Box::new(error))
    }
}
