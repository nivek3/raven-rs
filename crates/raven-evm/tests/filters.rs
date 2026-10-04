use raven_evm::{Address, B256, EvmFilter, Filter, Log};

/// Creates a deterministic address for filter constraints.
fn address(value: u8) -> Address {
    Address::from([value; 20])
}
/// Creates a deterministic topic hash for filter constraints.
fn topic(value: u8) -> B256 {
    B256::from([value; 32])
}

/// Builds a native RPC filter from explicit addresses and positional topics.
fn filter(addresses: Vec<Address>, topics: [Vec<B256>; 4]) -> Filter {
    Filter {
        address: addresses.into(),
        topics: topics.map(Into::into),
        ..Filter::default()
    }
}

#[test]
/// Verifies that native filter topics are positional and constraints require the topic to exist.
fn native_filter_topics_are_positional_and_constraints_require_the_topic_to_exist() {
    let filter = filter(
        vec![address(1)],
        [vec![topic(1)], vec![], vec![topic(2), topic(3)], vec![]],
    );
    assert!(
        filter.matches_address(address(1))
            && filter.matches_topics(&[topic(1), topic(9), topic(2)])
    );
    assert!(
        filter.matches_address(address(1))
            && filter.matches_topics(&[topic(1), topic(9), topic(3), topic(9)])
    );
    assert!(!filter.matches_address(address(2)));
    assert!(!filter.matches_topics(&[topic(2), topic(9), topic(1)]));
    assert!(!filter.matches_topics(&[topic(1)]));
}

#[test]
/// Verifies that native empty sets are wildcards and none means no log demand.
fn native_empty_sets_are_wildcards_and_none_means_no_log_demand() {
    let all = Filter::default();
    assert!(all.matches(&Log::new_unchecked(
        address(1),
        Vec::new(),
        Default::default()
    )));
    assert_eq!(EvmFilter::merge(&[]), EvmFilter::default());
    let all_logs = EvmFilter {
        blocks: false,
        logs: Some(all),
    };
    assert_eq!(
        EvmFilter::merge(&[EvmFilter::default(), all_logs.clone()]),
        all_logs
    );
}

#[test]
/// Verifies that coarse union retains every address and topic while wildcards dominate.
fn coarse_union_retains_every_address_and_topic_while_wildcards_dominate() {
    let a = filter(
        vec![address(2), address(1)],
        [vec![topic(1)], vec![], vec![], vec![]],
    );
    let b = filter(
        vec![address(2)],
        [vec![topic(2), topic(1), topic(2)], vec![], vec![], vec![]],
    );
    let merged = EvmFilter::merge(&[
        EvmFilter {
            blocks: false,
            logs: Some(a),
        },
        EvmFilter {
            blocks: false,
            logs: Some(b),
        },
    ])
    .logs
    .unwrap();
    assert_eq!(merged.address.len(), 2);
    assert!(merged.address.contains(&address(1)) && merged.address.contains(&address(2)));
    assert_eq!(merged.topics[0].len(), 2);
    assert!(merged.matches_address(address(1)) && merged.matches_topics(&[topic(2)]));

    let wildcard = Filter::default();
    let merged = EvmFilter::merge(&[
        EvmFilter {
            blocks: false,
            logs: Some(merged),
        },
        EvmFilter {
            blocks: false,
            logs: Some(wildcard),
        },
    ])
    .logs
    .unwrap();
    assert!(merged.address.is_empty());
    assert!(merged.topics.iter().all(|topic| topic.is_empty()));
}

#[test]
/// Verifies that merged filters drop parser block options for source owned binding.
fn merged_filters_drop_parser_block_options_for_source_owned_binding() {
    let left = Filter::new()
        .address(address(1))
        .event_signature(topic(2))
        .from_block(10u64);
    let right = left.clone().at_block_hash(topic(3));
    let merged = EvmFilter::merge(&[
        EvmFilter {
            blocks: true,
            logs: Some(left),
        },
        EvmFilter {
            blocks: false,
            logs: Some(right),
        },
    ]);
    assert!(merged.blocks);
    let merged = merged.logs.unwrap();
    assert_eq!(merged.extract_block_range(), (None, None));
    assert_eq!(merged.get_block_hash(), None);
    assert!(merged.address.contains(&address(1)));
    assert!(merged.topics[0].contains(&topic(2)));
}
