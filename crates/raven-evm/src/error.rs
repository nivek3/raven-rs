//! EVM normalization and pipeline configuration errors.

use raven_engine::RavenError;

/// Invalid EVM pipeline assembly detected before the engine starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PipelineError {
    #[error("at least one parser and its handlers must be registered")]
    NoParsers,
}

impl From<PipelineError> for RavenError {
    /// Classifies invalid pipeline assembly as a configuration error.
    fn from(error: PipelineError) -> Self {
        Self::Configuration(Box::new(error))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EvmError {
    #[error("a mined log is missing {0}")]
    MissingLogMetadata(&'static str),
    #[error("removed logs cannot be applied as canonical updates")]
    RemovedLog,
    #[error("an EVM log contains more than four topics")]
    TooManyTopics,
    #[error("a full block with transaction bodies is required")]
    IncompleteBlock,
    #[error("the log does not belong to its block")]
    LogIdentity,
    #[error("different logs occupy the same block log index")]
    ConflictingLog,
    #[error("log indices or transaction order are inconsistent")]
    LogPosition,
}

impl From<EvmError> for RavenError {
    /// Classifies EVM payload failures as parser errors.
    fn from(error: EvmError) -> Self {
        Self::Source(Box::new(error))
    }
}
