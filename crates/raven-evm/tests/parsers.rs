mod fake;

use raven_engine::RavenError;
use raven_evm::{
    BlockParser, Bytes, EvmFilter, Filter, Log, LogParser, LogUpdate, ParsedBlock, ParsedLog,
    Parser, SolEvent, U256, Update, build_block_batch,
};

use fake::{Changed, Notice, address, all, block, hash, log};

#[test]
/// Verifies that typed event parser preserves value emitter and all chain positions.
fn typed_event_parser_preserves_value_emitter_and_all_chain_positions() {
    let parser = LogParser::<Changed>::new(address(5));
    let input = Update::Log(LogUpdate::try_from(log(1, 4)).unwrap());
    let parsed: ParsedLog<Changed> = parser.parse(&input).unwrap().unwrap();
    assert_eq!(
        parsed.value,
        Changed {
            account: address(7),
            value: U256::from(42)
        }
    );
    assert_eq!(parsed.block_number, 100);
    assert_eq!(parsed.block_hash, hash(10));
    assert_eq!(parsed.transaction_hash, hash(2));
    assert_eq!(parsed.transaction_index, 1);
    assert_eq!(parsed.log_index, 4);
    assert_eq!(parsed.address, address(5));
    let filter = parser.filter();
    assert!(!filter.blocks);
    assert!(filter.logs.unwrap().topics[0].contains(&Changed::SIGNATURE_HASH));
}

#[test]
/// Verifies that other kinds addresses and signatures do not match.
fn other_kinds_addresses_and_signatures_do_not_match() {
    let parser = LogParser::<Changed>::new(address(5));
    assert!(
        parser
            .parse(&Update::Block(Box::new(block())))
            .unwrap()
            .is_none()
    );
    let mut other_address = log(0, 0);
    other_address.inner.address = address(99);
    let mut other_signature = log(0, 0);
    other_signature.inner.data.topics_mut()[0] = hash(99);
    for log in [other_address, other_signature] {
        assert!(
            parser
                .parse(&Update::Log(LogUpdate::try_from(log).unwrap()))
                .unwrap()
                .is_none()
        );
    }
}

#[test]
/// Verifies that matching malformed body topics and padding are parser errors.
fn matching_malformed_body_topics_and_padding_are_parser_errors() {
    let parser = LogParser::<Changed>::new(address(5));
    for kind in 0..5 {
        let mut log = LogUpdate::try_from(log(0, 0)).unwrap();
        let mut topics = log.log.topics().to_vec();
        let mut data = log.log.data.data.clone();
        match kind {
            0 => data = Bytes::new(),
            1 => {
                topics.pop();
            }
            2 => topics.push(hash(1)),
            3 => topics[1] = hash(255), // Nonzero address padding.
            _ => topics.extend([hash(1); 3]),
        }
        log.log = Log::new_unchecked(address(5), topics, data);
        assert!(matches!(
            parser.parse(&Update::Log(log)),
            Err(RavenError::Parser(_))
        ));
    }
}

#[test]
/// Verifies that custom indexed constraints remain exact and signature is intersected.
fn custom_indexed_constraints_remain_exact_and_signature_is_intersected() {
    let original = log(0, 0);
    let indexed = original.inner.topics()[1];
    let filter = Filter::new()
        .address(address(5))
        .event_signature(vec![hash(99), Changed::SIGNATURE_HASH])
        .topic1(indexed);
    let parser = LogParser::<Changed>::from_filter(filter);
    assert!(parser.filter().logs.unwrap().topics[0].contains(&Changed::SIGNATURE_HASH));
    let mut other = original.clone();
    other.inner.data = Changed {
        account: address(8),
        value: U256::from(1),
    }
    .encode_log_data();
    assert!(
        parser
            .parse(&Update::Log(LogUpdate::try_from(original).unwrap()))
            .unwrap()
            .is_some()
    );
    assert!(
        parser
            .parse(&Update::Log(LogUpdate::try_from(other).unwrap()))
            .unwrap()
            .is_none()
    );
    let parser = LogParser::<Changed>::from_filter(Filter::new().event_signature(hash(99)));
    assert!(parser.filter().logs.is_none());
    assert!(
        parser
            .parse(&Update::Log(LogUpdate::try_from(log(0, 0)).unwrap()))
            .unwrap()
            .is_none()
    );
}

#[test]
/// Verifies that anonymous events do not invent a signature topic.
fn anonymous_events_do_not_invent_a_signature_topic() {
    let parser = LogParser::<Notice>::new(address(5));
    let mut log = log(0, 0);
    log.inner.data = Notice {
        value: U256::from(7),
    }
    .encode_log_data();
    assert!(log.inner.topics().is_empty());
    assert!(
        parser
            .filter()
            .logs
            .unwrap()
            .topics
            .iter()
            .all(|topic| topic.is_empty())
    );
    let parsed = parser
        .parse(&Update::Log(LogUpdate::try_from(log).unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(parsed.value.value, U256::from(7));
    assert_eq!(parsed.log_index, 0);
}

#[test]
/// Verifies that block parser preserves full payload and has no log metadata.
fn block_parser_preserves_full_payload_and_has_no_log_metadata() {
    let block = block();
    let parsed: ParsedBlock<_> = BlockParser
        .parse(&Update::Block(Box::new(block.clone())))
        .unwrap()
        .unwrap();
    assert_eq!(parsed.value, block);
    assert_eq!(parsed.value.header.inner.timestamp, 1234);
    assert_eq!(parsed.value.transactions.len(), 2);
    assert_eq!(parsed.block_number, 100);
    assert_eq!(parsed.block_hash, hash(10));
    assert!(
        BlockParser
            .parse(&Update::Log(LogUpdate::try_from(log(0, 0)).unwrap()))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        BlockParser.filter(),
        EvmFilter {
            blocks: true,
            logs: None
        }
    );
}

#[test]
/// Verifies that block parser wraps the payload without revalidating transaction bodies.
fn block_parser_wraps_the_payload_without_revalidating_transaction_bodies() {
    let mut block = block();
    block.transactions.convert_to_hashes();
    let parsed = BlockParser
        .parse(&Update::Block(Box::new(block.clone())))
        .unwrap()
        .unwrap();
    assert_eq!(parsed.value, block);
    assert_eq!(parsed.value.transactions.len(), 2);
}

#[test]
/// Verifies that filtered and unfiltered batches deliver identical typed values in order.
fn filtered_and_unfiltered_batches_deliver_identical_typed_values_in_order() {
    let make_parser = |emitter, account| {
        let data = Changed {
            account,
            value: U256::ZERO,
        }
        .encode_log_data();
        LogParser::<Changed>::from_filter(Filter::new().address(emitter).topic1(data.topics()[1]))
    };
    let parsers = [
        make_parser(address(5), address(7)),
        make_parser(address(6), address(8)),
    ];
    let filters = EvmFilter::merge(&[
        BlockParser.filter(),
        parsers[0].filter(),
        parsers[1].filter(),
    ]);
    let mut raw = Vec::new();
    for (index, (emitter, account)) in [(5, 7), (5, 8), (6, 8), (99, 7), (5, 7)]
        .into_iter()
        .enumerate()
    {
        let mut log = log((index / 3) as u64, index as u64);
        log.inner.address = address(emitter);
        log.inner.data = Changed {
            account: address(account),
            value: U256::from(index as u64),
        }
        .encode_log_data();
        raw.push(log);
    }
    let unfiltered = build_block_batch(block(), raw.clone(), &all()).unwrap();
    let filtered = build_block_batch(block(), raw, &filters).unwrap();
    assert!(filtered.updates.len() < unfiltered.updates.len());
    let mut deliveries = Vec::new();
    for batch in [unfiltered, filtered] {
        let mut output = Vec::new();
        for update in &batch.updates {
            for (index, parser) in parsers.iter().enumerate() {
                if let Some(parsed) = parser.parse(update).unwrap() {
                    output.push((index, parsed));
                }
            }
        }
        deliveries.push(output);
    }
    assert_eq!(deliveries[0], deliveries[1]);
    assert_eq!(deliveries[0].len(), 3);
}
