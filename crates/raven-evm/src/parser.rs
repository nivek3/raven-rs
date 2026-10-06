//! Typed parsers that select normalized EVM updates and decode application values.

use std::marker::PhantomData;

use alloy_primitives::{Address, B256};
use alloy_rpc_types_eth::{Block, Filter};
use alloy_sol_types::SolEvent;
use raven_engine::{RavenError, RavenResult};

use crate::EvmFilter;
use crate::update::{LogUpdate, Update};

/// Typed log value with its required block, transaction and log position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLog<T> {
    pub value: T,
    pub block_number: u64,
    pub block_hash: B256,
    pub transaction_hash: B256,
    pub transaction_index: u64,
    pub log_index: u64,
    pub address: Address,
}

/// Typed block value with its block position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBlock<T> {
    pub value: T,
    pub block_number: u64,
    pub block_hash: B256,
}

impl LogUpdate {
    /// Attaches this log's canonical coordinates to a parsed application value.
    pub fn parsed<T>(&self, value: T) -> ParsedLog<T> {
        ParsedLog {
            value,
            block_number: self.block_number,
            block_hash: self.block_hash,
            transaction_hash: self.transaction_hash,
            transaction_index: self.transaction_index,
            log_index: self.log_index,
            address: self.log.address,
        }
    }
}

/// Selects and decodes EVM updates into typed application values.
/// Returns None for a non-match and an error for matching but malformed input.
/// Implementations must preserve relevant chain metadata and produce repeatable
/// results from the same update. Selection and decoding operate on the supplied
/// update in memory; perform asynchronous I/O in a source or handler.
pub trait Parser: Send + Sync {
    type Output: Send + Sync;

    /// Declares the source payload required by this parser.
    fn filter(&self) -> EvmFilter;

    /// Parses one normalized update, returning no value when it does not match.
    fn parse(&self, update: &Update) -> RavenResult<Option<Self::Output>>;
}

/// Typed ABI event parser with exact address/topic matching. For non-anonymous
/// events the event signature is always intersected with topic0. Anonymous events
/// have no signature discriminator: callers must supply sufficient address/topic
/// constraints; a matching payload which fails decoding is always an error.
pub struct LogParser<E> {
    filter: Option<Filter>,
    event: PhantomData<fn() -> E>,
}

impl<E: SolEvent> LogParser<E> {
    /// Matches one contract address and the event signature declared by `E`.
    pub fn new(address: Address) -> Self {
        Self::from_filter(Filter::new().address(address))
    }

    /// Builds a typed parser from an explicit log constraint.
    ///
    /// Non-anonymous events always intersect topic0 with their ABI signature.
    /// A conflicting topic0 therefore produces no log demand rather than
    /// broadening the query or decoding an unrelated event.
    pub fn from_filter(mut filter: Filter) -> Self {
        let includes_signature = E::ANONYMOUS
            || filter.topics[0].is_empty()
            || filter.topics[0].contains(&E::SIGNATURE_HASH);
        if !E::ANONYMOUS && includes_signature {
            filter.topics[0] = E::SIGNATURE_HASH.into();
        }
        Self {
            filter: includes_signature.then_some(filter),
            event: PhantomData,
        }
    }
}

impl<E: SolEvent + Send + Sync> Parser for LogParser<E> {
    type Output = ParsedLog<E>;

    /// Returns the exact log demand accepted by this ABI event parser.
    fn filter(&self) -> EvmFilter {
        EvmFilter {
            blocks: false,
            logs: self.filter.clone(),
        }
    }

    /// Decodes a matching log update and preserves its canonical coordinates.
    fn parse(&self, update: &Update) -> RavenResult<Option<ParsedLog<E>>> {
        let Update::Log(log) = update else {
            return Ok(None);
        };
        let Some(filter) = &self.filter else {
            return Ok(None);
        };
        let topics = log.log.topics();
        // Check selection before decoding; extra topics on an otherwise matching
        // malformed event must not be mistaken for a non-match.
        if !filter.matches_address(log.log.address)
            || !filter.matches_topics(&topics[..topics.len().min(4)])
        {
            return Ok(None);
        }
        let decoded = E::decode_log_validate(&log.log)
            .map_err(|error| RavenError::Parser(Box::new(error)))?;
        Ok(Some(log.parsed(decoded.data)))
    }
}

/// Wraps full block updates as `ParsedBlock<Block>` with their block position.
/// Sources must supply transaction bodies for block updates.
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockParser;

impl Parser for BlockParser {
    type Output = ParsedBlock<Block>;

    /// Requests complete block payloads without log updates.
    fn filter(&self) -> EvmFilter {
        EvmFilter {
            blocks: true,
            logs: None,
        }
    }

    /// Wraps a full block update with its block position.
    fn parse(&self, update: &Update) -> RavenResult<Option<ParsedBlock<Block>>> {
        let Update::Block(block) = update else {
            return Ok(None);
        };
        let block = block.as_ref();
        Ok(Some(ParsedBlock {
            value: block.clone(),
            block_number: block.header.inner.number,
            block_hash: block.header.hash,
        }))
    }
}
