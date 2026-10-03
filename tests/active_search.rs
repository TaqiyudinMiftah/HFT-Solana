use hft_solana::{
    active_store::ActivePoolStore,
    graph::{Cycle, Edge},
    quote::raydium::RaydiumFees,
    search::{capture_active_cycle, quote_active_cycle_consistent, SearchError},
    sizing::optimize_size_active,
    state::{CreatorFeeOn, PoolState, RaydiumCpmmState},
    types::{Direction, StateVersion},
};

fn state(slot: u64, generation: u64, a: u64, b: u64) -> PoolState {
    PoolState::RaydiumCpmm(RaydiumCpmmState {
        version: StateVersion {
            slot,
            write_version: generation,
            generation,
        },
        reserve_a: a,
        reserve_b: b,
        fees: RaydiumFees {
            trade_fee_rate: 0,
            creator_fee_rate: 0,
            protocol_fee_rate: 0,
            fund_fee_rate: 0,
        },
        creator_fee_on: CreatorFeeOn::Both,
    })
}

fn cycle() -> Cycle {
    Cycle {
        id: 0,
        len: 2,
        edges: [
            Edge {
                pool: 0,
                direction: Direction::AtoB,
                from_token: 0,
                to_token: 1,
            },
            Edge {
                pool: 1,
                direction: Direction::BtoA,
                from_token: 1,
                to_token: 0,
            },
            Edge {
                pool: 0,
                direction: Direction::AtoB,
                from_token: 0,
                to_token: 0,
            },
        ],
        start_token: 0,
    }
}

#[test]
fn inactive_pool_blocks_cycle_quote() {
    let mut store = ActivePoolStore::new(2, 2);
    store.publish(0, state(100, 1, 10_000_000, 20_000_000));

    let err = quote_active_cycle_consistent(&cycle(), &store, 1_000, 0).unwrap_err();
    assert_eq!(err, SearchError::InactivePool(1));
}

#[test]
fn captured_snapshot_detects_replacement_before_emit() {
    let mut store = ActivePoolStore::new(2, 2);
    store.publish(0, state(100, 1, 10_000_000, 20_000_000));
    store.publish(1, state(100, 1, 10_000_000, 20_000_000));

    let snapshot = capture_active_cycle(&cycle(), &store, 0).unwrap();
    let _ = snapshot.quote(&cycle(), 1_000).unwrap();

    store.publish(1, state(100, 2, 10_000_000, 20_000_000));

    assert!(matches!(
        snapshot.validate(&store),
        Err(SearchError::StaleGeneration { pool: 1, .. })
    ));
}

#[test]
fn active_sizing_uses_one_consistent_snapshot() {
    let mut store = ActivePoolStore::new(2, 2);
    store.publish(0, state(100, 1, 10_000_000, 21_000_000));
    store.publish(1, state(100, 1, 10_000_000, 20_000_000));

    let best = optimize_size_active(&cycle(), &store, 1_000, 10_000, 0)
        .unwrap()
        .unwrap();

    assert!(best.amount_in > 0);
}
