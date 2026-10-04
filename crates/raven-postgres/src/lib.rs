//! PostgreSQL storage: one dedicated schema and writer session per index.
mod error;
mod storage;
mod store;

pub use error::PostgresError;
pub use storage::PostgresStorage;
pub use store::PostgresChainStore;
