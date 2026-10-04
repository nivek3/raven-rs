use raven_engine::RavenError;

#[derive(Debug, thiserror::Error)]
pub enum RpcLogError {
    #[error("an HTTP or HTTPS RPC endpoint is required")]
    InvalidEndpoint,
    #[error("Alloy RPC request failed")]
    Request(#[source] alloy_transport::TransportError),
    #[error("the sequential batch receiver was closed")]
    ReceiverClosed,
}

impl From<RpcLogError> for RavenError {
    /// Classifies RPC log crawler failures as datasource errors.
    fn from(error: RpcLogError) -> Self {
        Self::Source(Box::new(error))
    }
}
