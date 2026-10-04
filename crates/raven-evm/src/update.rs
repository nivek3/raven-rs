//! EVM updates and block batch construction.

use std::collections::BTreeMap;

use alloy_primitives::{B256, Log};
use alloy_rpc_types_eth::{Block, Log as RpcLog};
use raven_engine::RavenResult;

use crate::{EvmError, EvmFilter};

pub type LiteBlockHeader = raven_engine::LiteBlockHeader<B256>;
pub type BlockBatch = raven_engine::BlockBatch<B256, Update>;

/// EVM data delivered within a block batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Block(BlockUpdate),
    Log(LogUpdate),
}

/// Full Ethereum block payload, including transaction bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockUpdate {
    pub block: Block,
}

/// Mined log with its block, transaction and global log coordinates.
/// RPC conversion requires these coordinates and rejects removed notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogUpdate {
    pub log: Log,
    pub block_number: u64,
    pub block_hash: B256,
    pub transaction_hash: B256,
    pub transaction_index: u64,
    pub log_index: u64,
}

impl TryFrom<RpcLog> for LogUpdate {
    type Error = EvmError;

    /// Converts a canonical RPC log after requiring all position metadata.
    fn try_from(log: RpcLog) -> Result<Self, Self::Error> {
        if log.removed {
            return Err(EvmError::RemovedLog);
        }
        if log.topics().len() > 4 {
            return Err(EvmError::TooManyTopics);
        }
        Ok(Self {
            block_number: log
                .block_number
                .ok_or(EvmError::MissingLogMetadata("block number"))?,
            block_hash: log
                .block_hash
                .ok_or(EvmError::MissingLogMetadata("block hash"))?,
            transaction_hash: log
                .transaction_hash
                .ok_or(EvmError::MissingLogMetadata("transaction hash"))?,
            transaction_index: log
                .transaction_index
                .ok_or(EvmError::MissingLogMetadata("transaction index"))?,
            log_index: log
                .log_index
                .ok_or(EvmError::MissingLogMetadata("log index"))?,
            log: log.inner,
        })
    }
}

/// Builds a processing-ready batch from a block and its associated RPC logs.
/// Sources establish log completeness and block association, including for empty
/// responses. The builder checks payload requirements and log identity, coalesces
/// identical logs, and applies the filter. A selected block precedes logs ordered
/// by log index; transaction indices must follow the same order.
pub fn build_block_batch(
    block: Block,
    logs: Vec<RpcLog>,
    filter: &EvmFilter,
) -> RavenResult<BlockBatch> {
    if block.transactions.is_uncle() || (filter.blocks && !block.transactions.is_full()) {
        return Err(EvmError::IncompleteBlock.into());
    }
    let header = LiteBlockHeader {
        number: block.header.inner.number,
        hash: block.header.hash,
        parent_hash: block.header.inner.parent_hash,
    };
    let mut unique = BTreeMap::new();
    for log in logs {
        let log = LogUpdate::try_from(log)?;
        if log.block_number != header.number || log.block_hash != header.hash {
            return Err(EvmError::LogIdentity.into());
        }
        match unique.get(&log.log_index) {
            Some(existing) if existing != &log => return Err(EvmError::ConflictingLog.into()),
            Some(_) => {}
            None => {
                unique.insert(log.log_index, log);
            }
        }
    }
    let mut updates: Vec<Update> = Vec::new();
    if filter.blocks {
        updates.push(Update::Block(BlockUpdate { block }));
    }
    // The map orders unique global log indices; transaction order must agree.
    let mut previous_transaction = 0;
    for log in unique.into_values() {
        if log.transaction_index < previous_transaction {
            return Err(EvmError::LogPosition.into());
        }
        previous_transaction = log.transaction_index;
        if filter
            .logs
            .as_ref()
            .is_some_and(|filter| filter.matches(&log.log))
        {
            updates.push(Update::Log(log));
        }
    }
    Ok(BlockBatch { header, updates })
}
