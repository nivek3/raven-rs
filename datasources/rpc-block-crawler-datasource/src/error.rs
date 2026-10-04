use raven_engine::RavenError;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("an HTTP or HTTPS RPC endpoint is required")]
    InvalidEndpoint,
    #[error("invalid RPC block crawler configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("Alloy RPC request failed")]
    Request(#[source] alloy_transport::TransportError),
    #[error("the sequential batch receiver was closed")]
    ReceiverClosed,
}

impl From<RpcError> for RavenError {
    /// Classifies RPC crawler failures as datasource errors.
    fn from(error: RpcError) -> Self {
        Self::Source(Box::new(error))
    }
}
