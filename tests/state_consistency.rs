use hft_solana::{
    graph::{Cycle, Edge},
    quote::raydium::RaydiumFees,
    search::{quote_cycle_consistent, SearchError},
    state::{CreatorFeeOn, PoolCell, PoolState, RaydiumCpmmState},
    types::{Direction, StateVersion},
};

fn pool(slot: u64, generation: u64) -> PoolCell {
    PoolCell::new(PoolState::RaydiumCpmm(RaydiumCpmmState {
        version: StateVersion {
            slot,
            write_version: 1,
            generation,
        },
        reserve_a: 10_000_000,
        reserve_b: 20_000_000,
        fees: RaydiumFees {
            trade_fee_rate: 0,
            creator_fee_rate: 0,
            protocol_fee_rate: 0,
            fund_fee_rate: 0,
        },
        creator_fee_on: CreatorFeeOn::Both,
    }))
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
fn rejects_cycle_with_excessive_slot_skew() {
    let pools = vec![pool(100, 1), pool(103, 1)];

    let err = quote_cycle_consistent(&cycle(), &pools, 1_000, 1).unwrap_err();
    assert_eq!(
        err,
        SearchError::SlotSkew {
            min_slot: 100,
            max_slot: 103,
            max_allowed: 1,
        }
    );
}

#[test]
fn accepts_cycle_within_slot_skew_limit() {
    let pools = vec![pool(100, 1), pool(101, 1)];

    let result = quote_cycle_consistent(&cycle(), &pools, 1_000, 1).unwrap();
    assert!(result.amount_out > 0);
}
