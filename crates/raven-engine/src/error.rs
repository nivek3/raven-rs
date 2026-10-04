//! Errors shared by the chain-neutral engine contracts.
//!
//! Chain-specific crates keep their own concrete error enums and wrap them in
//! the appropriate framework category at the interface.

/// Shared result for Raven framework interfaces. Local value types may retain
/// specific error types and convert them at the framework interface with `?`.
pub type RavenResult<T> = Result<T, RavenError>;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum RavenError {
    #[error("configuration error: {0}")]
    Configuration(#[source] BoxError),

    #[error("parser error: {0}")]
    Parser(#[source] BoxError),

    #[error("handler error: {0}")]
    Handler(#[source] BoxError),

    #[error(transparent)]
    Entity(#[from] EntityError),

    #[error(transparent)]
    Engine(#[from] EngineError),

    #[error("chain store error: {0}")]
    ChainStore(#[source] BoxError),

    #[error("processor error: {0}")]
    Processor(#[source] BoxError),

    #[error(transparent)]
    Position(#[from] PositionError),

    /// Preserve the underlying acquisition/transport error for inspection.
    #[error("source error: {0}")]
    Source(#[source] BoxError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PositionError {
    #[error("the start parent must immediately precede the start block")]
    InvalidStartBlock,
    #[error("a committed block pointer requires persisted network metadata")]
    MissingNetwork,
    #[error("the block pointer precedes the indexing start block")]
    BlockPtrBeforeStartBlock,
    #[error("the next block number exceeds u64")]
    HeightOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("source chain identity differs from the persisted network metadata")]
    ChainIdentityMismatch,
    #[error("source has not established a head sufficient to advance this index")]
    SourceBehind,
    #[error("the selected branch changed during resynchronization")]
    SourceChanged,
    #[error("a required block is unavailable")]
    MissingBlock,
    #[error("block identity, payload or parent relationship is inconsistent")]
    InvalidBlock,
    #[error("the fork crosses the persisted indexing start; rebuild required")]
    ReorgBeforeStartBlock,
    #[error("required local canonical history is missing or inconsistent")]
    InvalidLocalHistory,
    #[error("committed block pointer changed before the transition")]
    BlockPtrConflict,
    #[error("the entity state has a failed or interrupted read; discard this block")]
    EntityStateFailed,
    #[error("poll interval and channel capacity must be positive")]
    InvalidRunOptions,
    #[error("the datasource producer task is missing")]
    MissingProducer,
}

/// Failures converting application entities through the typed store interface.
#[derive(Debug, thiserror::Error)]
pub enum EntityError {
    #[error("failed to decode {entity_type} entity: {source}")]
    Decode {
        entity_type: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to encode {entity_type} entity: {source}")]
    Encode {
        entity_type: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("{entity_type} entity ID mismatch: expected {expected}, got {actual}")]
    IdMismatch {
        entity_type: &'static str,
        expected: String,
        actual: String,
    },
}
