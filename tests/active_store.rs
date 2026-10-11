use hft_solana::{
    active_store::ActivePoolStore,
    quote::raydium::RaydiumFees,
    reactor::{ReactorInvalidation, ReactorOutput},
    state::{CreatorFeeOn, PoolState, RaydiumCpmmState},
    types::StateVersion,
    update_queue::MarkDirtyResult,
};

fn state(generation: u64) -> PoolState {
    PoolState::RaydiumCpmm(RaydiumCpmmState {
        version: StateVersion {
            slot: 100,
            write_version: generation,
            generation,
        },
        reserve_a: 1_000,
        reserve_b: 2_000,
        fees: RaydiumFees {
            trade_fee_rate: 0,
            creator_fee_rate: 0,
            protocol_fee_rate: 0,
            fund_fee_rate: 0,
        },
        creator_fee_on: CreatorFeeOn::Both,
    })
}

#[test]
fn updated_pool_becomes_active_and_dirty() {
    let mut store = ActivePoolStore::new(2, 2);

    assert_eq!(
        store.apply(ReactorOutput::PoolUpdated {
            pool_id: 0,
            state: state(1),
        }),
        MarkDirtyResult::Queued
    );

    assert!(store.is_ready(0));
    assert_eq!(store.pop_dirty(), Some(0));
}

#[test]
fn invalidation_removes_pool_and_requeues_cycles() {
    let mut store = ActivePoolStore::new(2, 2);
    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 1,
        state: state(1),
    });
    assert_eq!(store.pop_dirty(), Some(1));
    assert!(store.is_ready(1));

    assert_eq!(
        store.apply(ReactorOutput::PoolInvalidated {
            pool_id: 1,
            reason: ReactorInvalidation::Snapshot("test".to_owned()),
        }),
        MarkDirtyResult::Queued
    );

    assert!(!store.is_ready(1));
    assert_eq!(store.pop_dirty(), Some(1));
}

#[test]
fn multiple_updates_to_same_active_pool_coalesce_dirty_work() {
    let mut store = ActivePoolStore::new(1, 1);

    assert_eq!(
        store.apply(ReactorOutput::PoolUpdated {
            pool_id: 0,
            state: state(1),
        }),
        MarkDirtyResult::Queued
    );
    assert_eq!(
        store.apply(ReactorOutput::PoolUpdated {
            pool_id: 0,
            state: state(2),
        }),
        MarkDirtyResult::AlreadyDirty
    );

    let current = store.get(0).unwrap().version();
    assert_eq!(current.generation, 2);
    assert_eq!(store.pop_dirty(), Some(0));
}
