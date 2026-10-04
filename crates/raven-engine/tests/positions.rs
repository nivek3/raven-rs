mod fake;

use raven_engine::{BlockPtr, Network, PositionError, next_block_number};

use crate::fake::{FakeStore, block};

#[test]
/// Verifies that persisted start survives full rollback and configuration changes.
fn persisted_start_survives_full_rollback_and_configuration_changes() {
    let parent = block(49, 490, 480, vec![]).header;
    let network = Network::new(1, 50, Some(parent)).unwrap();
    let store = FakeStore::new(network);
    let first = block(50, 500, 490, vec![]).header;
    store.commit_empty_block(first.clone()).unwrap();
    assert_eq!(
        next_block_number(900, store.network().as_ref(), store.block_ptr().as_ref()).unwrap(),
        51
    );
    store.revert_block(&first).unwrap();
    assert_eq!(store.block_ptr(), None);
    assert_eq!(
        next_block_number(900, store.network().as_ref(), None).unwrap(),
        50
    );
    assert_eq!(store.network().unwrap().chain_identity(), &1);
    assert_eq!(store.network().unwrap().start_block(), 50);
    assert_eq!(store.network().unwrap().parent().unwrap().hash, 490);
}

#[test]
/// Verifies that invalid persisted positions fail instead of wrapping or resetting.
fn invalid_persisted_positions_fail_instead_of_wrapping_or_resetting() {
    let network = Network::<u64, _>::new(1, 50, Some(block(49, 49, 48, vec![]).header)).unwrap();
    assert_eq!(
        Network::<u64, _>::new(1, 50, None),
        Err(PositionError::InvalidStartBlock)
    );
    assert_eq!(
        Network::new(1, 0, Some(block(u64::MAX, 1, 0, vec![]).header)),
        Err(PositionError::InvalidStartBlock)
    );
    assert_eq!(
        next_block_number(
            0,
            Some(&network),
            Some(&BlockPtr {
                number: 49,
                hash: 49
            })
        ),
        Err(PositionError::BlockPtrBeforeStartBlock)
    );
    assert_eq!(
        next_block_number(
            0,
            Some(&network),
            Some(&BlockPtr {
                number: u64::MAX,
                hash: 1
            })
        ),
        Err(PositionError::HeightOverflow)
    );
    assert_eq!(
        next_block_number::<u64, u64>(
            0,
            None,
            Some(&BlockPtr {
                number: 100,
                hash: 1
            })
        ),
        Err(PositionError::MissingNetwork)
    );
    assert_eq!(next_block_number::<u64, u64>(50, None, None).unwrap(), 50);
}

#[test]
/// Verifies that first block requires start block height and parent with a genesis exception.
fn first_block_requires_start_block_height_and_parent_with_a_genesis_exception() {
    let network = Network::new(1, 50, Some(block(49, 490, 480, vec![]).header)).unwrap();
    assert!(network.accepts_first(&block(50, 500, 490, vec![]).header));
    assert!(!network.accepts_first(&block(51, 500, 490, vec![]).header));
    assert!(!network.accepts_first(&block(50, 500, 999, vec![]).header));
    let genesis = Network::<u64, _>::new(1, 0, None).unwrap();
    assert!(genesis.accepts_first(&block(0, 10, 0, vec![]).header));
}

#[test]
/// Verifies that an incorrect revert leaves committed metadata unchanged.
fn wrong_revert_leave_committed_metadata_unchanged() {
    let network = Network::new(1, 1, Some(block(0, 10, 0, vec![]).header)).unwrap();
    let store = FakeStore::new(network);
    let a = block(1, 11, 10, vec![]).header;
    let b = block(2, 12, 11, vec![]).header;
    assert_eq!(store.block_ptr(), None);
    assert_eq!(store.block_header_by_number(1), None);
    store.commit_empty_block(a.clone()).unwrap();
    assert_eq!(store.block_ptr(), Some(BlockPtr::from(&a)));
    assert_eq!(store.block_header_by_number(2), None);
    assert!(store.revert_block(&b).is_err());
    store.commit_empty_block(b.clone()).unwrap();
    assert!(store.revert_block(&a).is_err());
    assert_eq!(store.block_ptr(), Some(BlockPtr::from(&b)));
    store.revert_block(&b).unwrap();
    store.commit_empty_block(b.clone()).unwrap();
    assert_eq!(store.block_ptr(), Some(BlockPtr::from(&b)));
}
