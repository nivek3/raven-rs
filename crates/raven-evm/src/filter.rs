//! Concrete EVM acquisition filters shared by pipelines and RPC sources.

use alloy_rpc_types_eth::{Filter, FilterSet};

/// Selects full block updates and matching log updates. Sources retain headers
/// for canonical tracking; `blocks` controls delivery of block updates.
/// Default requests no updates. `Some(Filter::default())` requests every log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvmFilter {
    pub blocks: bool,
    pub logs: Option<Filter>,
}

impl EvmFilter {
    /// Merges parser demand by taking a conservative union of addresses and topics.
    /// An empty slice selects no updates. Sources choose the block range or hash.
    pub fn merge(filters: &[Self]) -> Self {
        let logs: Vec<_> = filters
            .iter()
            .filter_map(|filter| filter.logs.as_ref())
            .collect();
        let logs = (!logs.is_empty()).then(|| Filter {
            address: merge_set(logs.iter().map(|filter| &filter.address)),
            topics: std::array::from_fn(|index| {
                merge_set(logs.iter().map(|filter| &filter.topics[index]))
            }),
            ..Filter::default()
        });
        Self {
            blocks: filters.iter().any(|filter| filter.blocks),
            logs,
        }
    }
}

/// Unions one positional filter set, preserving a wildcard when any input has one.
fn merge_set<'a, T: Clone + Eq + std::hash::Hash + 'a>(
    sets: impl Iterator<Item = &'a FilterSet<T>>,
) -> FilterSet<T> {
    let mut values = Vec::new();
    for set in sets {
        if set.is_empty() {
            return FilterSet::default();
        }
        values.extend(set.iter().cloned());
    }
    values.into()
}
