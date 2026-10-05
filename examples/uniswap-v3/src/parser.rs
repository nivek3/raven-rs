//! Select candidate logs; decode only after checking application pool membership.
use crate::events::*;
use alloy_primitives::Address;
use alloy_rpc_types_eth::Filter;
use alloy_sol_types::SolEvent;
use raven_engine::RavenResult;
use raven_evm::{EvmFilter, LogUpdate, Parser, Update};

pub(crate) struct UniswapParser {
    factory: Address,
    filters: [Filter; 2],
}

/// Selects every pool-side event relevant to the mapping.
pub(crate) fn pool_filter() -> Filter {
    Filter::new().event_signature(vec![
        PoolCreated::SIGNATURE_HASH,
        Initialize::SIGNATURE_HASH,
        Mint::SIGNATURE_HASH,
        Burn::SIGNATURE_HASH,
        Swap::SIGNATURE_HASH,
        Flash::SIGNATURE_HASH,
    ])
}

/// Selects position-manager NFT events from the configured address.
pub(crate) fn position_filter(manager: Address) -> Filter {
    Filter::new().address(manager).event_signature(vec![
        PositionManager::IncreaseLiquidity::SIGNATURE_HASH,
        PositionManager::DecreaseLiquidity::SIGNATURE_HASH,
        PositionManager::Collect::SIGNATURE_HASH,
        PositionManager::Transfer::SIGNATURE_HASH,
    ])
}

impl UniswapParser {
    /// Builds the candidate-log selector for one factory and position manager.
    pub(crate) fn new(factory: Address, manager: Address) -> Self {
        Self {
            factory,
            filters: [pool_filter(), position_filter(manager)],
        }
    }
}

impl Parser for UniswapParser {
    type Output = LogUpdate;
    /// Declares the union of pool and position-manager log requirements.
    fn filter(&self) -> EvmFilter {
        EvmFilter::merge(&self.filters.clone().map(|logs| EvmFilter {
            blocks: false,
            logs: Some(logs),
        }))
    }
    /// Keeps only subscribed logs whose factory event comes from the configured factory.
    fn parse(&self, update: &Update) -> RavenResult<Option<LogUpdate>> {
        let Update::Log(log) = update else {
            return Ok(None);
        };
        if !self.filters.iter().any(|filter| filter.matches(&log.log)) {
            return Ok(None);
        }
        if log.log.topics().first() == Some(&PoolCreated::SIGNATURE_HASH)
            && log.log.address != self.factory
        {
            return Ok(None);
        }
        Ok(Some(log.clone()))
    }
}
