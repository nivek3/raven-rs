mod fake;

use raven_evm::{
    BlockUpdate, EvmError, EvmFilter, Filter, Log, LogUpdate, Update, build_block_batch,
};

use fake::{address, all, assert_error, block, hash, header, log};

#[test]
/// Verifies that normalization sorts deduplicates and emits full block first.
fn normalization_sorts_deduplicates_and_emits_full_block_first() {
    let block = block();
    let logs = vec![log(1, 4), log(0, 2), log(0, 0), log(0, 2)];
    let batch = build_block_batch(block.clone(), logs, &all()).unwrap();
    assert_eq!(batch.header, header());
    assert_eq!(batch.updates[0], Update::Block(BlockUpdate { block }));
    let positions: Vec<_> = batch.updates[1..]
        .iter()
        .map(|update| match update {
            Update::Log(log) => (log.transaction_index, log.log_index),
            _ => panic!("block must occur only once"),
        })
        .collect();
    assert_eq!(positions, vec![(0, 0), (0, 2), (1, 4)]);
}

#[test]
/// Verifies that no demand or no matching logs still produces a header.
fn no_demand_or_no_matching_logs_still_produces_a_header() {
    for filters in [
        EvmFilter::default(),
        EvmFilter {
            blocks: false,
            logs: Some(Filter::new().address(address(99))),
        },
    ] {
        let batch = build_block_batch(block(), vec![log(0, 0)], &filters).unwrap();
        assert_eq!(batch.header, header());
        assert!(batch.updates.is_empty());
    }
}

#[test]
/// Verifies that empty updates preserve the header derived from the block.
fn empty_updates_preserve_the_header_derived_from_the_block() {
    let mut block = block();
    block.header.hash = hash(99);
    block.header.inner.number = 199;
    block.header.inner.parent_hash = hash(98);
    let batch = build_block_batch(block, vec![], &EvmFilter::default()).unwrap();
    assert_eq!(batch.header.hash, hash(99));
    assert_eq!(batch.header.number, 199);
    assert_eq!(batch.header.parent_hash, hash(98));
    assert!(batch.updates.is_empty());
}

#[test]
/// Verifies that mandatory log metadata cannot be invented.
fn mandatory_log_metadata_cannot_be_invented() {
    let fields = [
        "block number",
        "block hash",
        "transaction hash",
        "transaction index",
        "log index",
    ];
    for (index, name) in fields.into_iter().enumerate() {
        let mut log = log(0, 0);
        match index {
            0 => log.block_number = None,
            1 => log.block_hash = None,
            2 => log.transaction_hash = None,
            3 => log.transaction_index = None,
            _ => log.log_index = None,
        }
        assert_eq!(
            LogUpdate::try_from(log).unwrap_err(),
            EvmError::MissingLogMetadata(name)
        );
    }
}

#[test]
/// Verifies that removed and overlong logs are rejected.
fn removed_and_overlong_logs_are_rejected() {
    let mut removed = log(0, 0);
    removed.removed = true;
    assert_eq!(
        LogUpdate::try_from(removed).unwrap_err(),
        EvmError::RemovedLog
    );
    let mut overlong = log(0, 0);
    overlong.inner = Log::new_unchecked(address(5), vec![hash(1); 5], Default::default());
    assert_eq!(
        LogUpdate::try_from(overlong).unwrap_err(),
        EvmError::TooManyTopics
    );
}

#[test]
/// Verifies that rpc log timestamp is not cross checked.
fn rpc_log_timestamp_is_not_cross_checked() {
    let mut log = log(0, 0);
    log.block_timestamp = Some(9999);
    let batch = build_block_batch(block(), vec![log], &all()).unwrap();
    let Update::Log(log) = &batch.updates[1] else {
        panic!("full block update must precede the log");
    };
    assert_eq!(log.log_index, 0);
}

#[test]
/// Verifies that log transaction metadata is preserved without cross checking block transactions.
fn log_transaction_metadata_is_preserved_without_cross_checking_block_transactions() {
    for field in 0..2 {
        let mut log = log(0, 0);
        if field == 0 {
            log.transaction_hash = Some(hash(99));
        } else {
            log.transaction_index = Some(u64::MAX);
        }
        let expected_hash = log.transaction_hash;
        let expected_index = log.transaction_index;
        let batch = build_block_batch(block(), vec![log], &all()).unwrap();
        let Update::Log(update) = &batch.updates[1] else {
            panic!("full block update must precede the log");
        };
        assert_eq!(Some(update.transaction_hash), expected_hash);
        assert_eq!(Some(update.transaction_index), expected_index);
    }
}

#[test]
/// Verifies that conflicting duplicates and global log index regressions are rejected.
fn conflicting_duplicates_and_global_log_index_regressions_are_rejected() {
    let mut conflicting = log(0, 0);
    conflicting.inner.address = address(99);
    assert_error(
        build_block_batch(block(), vec![log(0, 0), conflicting], &all()),
        EvmError::ConflictingLog,
    );
    for filter in [all(), EvmFilter::default()] {
        assert_error(
            build_block_batch(block(), vec![log(0, 5), log(1, 3)], &filter),
            EvmError::LogPosition,
        );
    }
}

#[test]
/// Verifies that full block filters reject hash only or uncle payloads.
fn full_block_filters_reject_hash_only_or_uncle_payloads() {
    let mut hashes = block();
    hashes.transactions.convert_to_hashes();
    assert_error(
        build_block_batch(hashes.clone(), vec![], &all()),
        EvmError::IncompleteBlock,
    );
    let filters = EvmFilter {
        blocks: false,
        logs: all().logs,
    };
    build_block_batch(hashes, vec![log(0, 0)], &filters).unwrap();
    let mut uncle = block();
    let uncle_payload: raven_evm::Block =
        alloy_rpc_types_eth::Block::uncle_from_header(uncle.header.inner.clone());
    uncle.transactions = uncle_payload.transactions;
    assert_error(
        build_block_batch(uncle, vec![], &filters),
        EvmError::IncompleteBlock,
    );
}

#[test]
/// Verifies that full transaction metadata and duplicate hashes are trusted from the source.
fn full_transaction_metadata_and_duplicate_hashes_are_trusted_from_the_source() {
    let mut invalid = block();
    let mut transactions = invalid.transactions.clone().into_transactions_vec();
    transactions[0].block_hash = Some(hash(99));
    invalid.transactions = transactions.into();
    build_block_batch(invalid, vec![], &all()).unwrap();
    let mut duplicate = block();
    duplicate.transactions = vec![hash(1), hash(1)].into();
    build_block_batch(duplicate, vec![], &EvmFilter::default()).unwrap();
}
