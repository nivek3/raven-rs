//! Canonical indexing kernel: reorg-safe ingestion, source contracts and store transactions.
//!
//! Sources deliver complete block batches. The engine applies Head or Confirmations
//! policy, resynchronizes canonical state, and executes a `BlockProcessor` against an
//! entity state. Final changes and indexing progress are committed in one
//! store transaction per block.

mod engine;
mod entity;
mod error;
mod finality;
mod handler;
mod pipeline;
mod source;
mod store;
mod types;

pub use engine::{Engine, IngestOutcome};
pub use entity::{Entity, EntityChange, EntityStore, EntityStoreExt, EntityValue};
pub use error::{EngineError, EntityError, PositionError, RavenError, RavenResult};
pub use finality::FinalityPolicy;
pub use handler::{Handler, Handlers};
pub use pipeline::RunOptions;
pub use source::{BlockSource, CancellationToken, Datasource, Sender};
pub use store::{BlockProcessor, ChainStore};
pub use types::{BlockBatch, BlockPtr, LiteBlockHeader, Network, next_block_number};
