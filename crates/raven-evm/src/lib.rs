//! Normalized EVM batches, filters, typed ABI parsers and Pipeline.

mod error;
mod filter;
mod parser;
mod pipeline;
mod update;

pub use alloy_primitives::{Address, B256, Bytes, Log, U256};
pub use alloy_rpc_types_eth::{Block, Filter, FilterSet, Log as RpcLog, Topic};
pub use alloy_sol_types::{SolEvent, sol};
pub use error::{EvmError, PipelineError};
pub use filter::EvmFilter;
pub use parser::{BlockParser, LogParser, Parsed, Parser};
pub use pipeline::{Pipeline, PipelineBuilder};
pub use update::{BlockBatch, BlockUpdate, LiteBlockHeader, LogUpdate, Update, build_block_batch};
