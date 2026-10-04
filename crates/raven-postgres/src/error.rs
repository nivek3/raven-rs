use raven_engine::RavenError;

#[derive(Debug, thiserror::Error)]
pub enum PostgresError {
    #[error(
        "schema must be a lowercase ASCII identifier of 1–63 bytes; public and pg_* are reserved"
    )]
    InvalidSchema,
    #[error("network name must not be empty")]
    InvalidNetworkName,
    #[error("the schema already has a writer")]
    WriterBusy,
    #[error("the PostgreSQL writer session no longer owns its lock; reopen the store")]
    WriterLost,
    #[error("PostgreSQL operation failed")]
    Database(#[from] sqlx::Error),
    #[error("persisted state is inconsistent")]
    InvalidState,
    #[error("the persisted network metadata differs from the requested metadata")]
    NetworkConflict,
}

impl From<PostgresError> for RavenError {
    /// Classifies PostgreSQL storage failures as chain-store errors.
    fn from(error: PostgresError) -> Self {
        Self::ChainStore(Box::new(error))
    }
}

/// Wraps a SQLx failure in Raven's chain-store error variant.
pub(crate) fn database(error: sqlx::Error) -> RavenError {
    PostgresError::Database(error).into()
}
