use hft_solana::{
    active_store::ActivePoolStore,
    graph::{Cycle, Edge, GraphIndex},
    opportunity_engine::{CycleSearchConfig, OpportunityEngine},
    quote::raydium::RaydiumFees,
    reactor::ReactorOutput,
    state::{CreatorFeeOn, PoolState, RaydiumCpmmState},
    types::{Direction, StateVersion},
};

fn pool_state(generation: u64, reserve_a: u64, reserve_b: u64) -> PoolState {
    PoolState::RaydiumCpmm(RaydiumCpmmState {
        version: StateVersion {
            slot: 100,
            write_version: generation,
            generation,
        },
        reserve_a,
        reserve_b,
        fees: RaydiumFees {
            trade_fee_rate: 0,
            creator_fee_rate: 0,
            protocol_fee_rate: 0,
            fund_fee_rate: 0,
        },
        creator_fee_on: CreatorFeeOn::Both,
    })
}

fn graph() -> GraphIndex {
    GraphIndex::from_cycles(
        2,
        vec![Cycle {
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
        }],
    )
}

fn config() -> CycleSearchConfig {
    CycleSearchConfig {
        initial_seed: 1_000,
        minimum_probe: 100,
        max_size: 10_000,
        minimum_effective_profit: 1,
        max_slot_skew: 0,
        expected_cu: 120_000,
    }
}

#[test]
fn same_snapshot_is_evaluated_only_once_across_dirty_pools() {
    let mut store = ActivePoolStore::new(2, 2);

    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 0,
        state: pool_state(1, 10_000_000, 22_000_000),
    });
    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 1,
        state: pool_state(1, 10_000_000, 20_000_000),
    });

    let mut engine = OpportunityEngine::new(graph(), vec![config()]);

    let first_dirty = store.pop_dirty().unwrap();
    let first = engine.process_pool(first_dirty, &store, 123);
    assert_eq!(first.len(), 1);
    assert!(first[0].expected_effective_profit > 0);

    let second_dirty = store.pop_dirty().unwrap();
    let second = engine.process_pool(second_dirty, &store, 124);
    assert!(second.is_empty());
}

#[test]
fn new_pool_generation_reopens_cycle_for_evaluation() {
    let mut store = ActivePoolStore::new(2, 2);

    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 0,
        state: pool_state(1, 10_000_000, 22_000_000),
    });
    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 1,
        state: pool_state(1, 10_000_000, 20_000_000),
    });

    let mut engine = OpportunityEngine::new(graph(), vec![config()]);

    while let Some(pool) = store.pop_dirty() {
        let _ = engine.process_pool(pool, &store, 100);
    }

    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 0,
        state: pool_state(2, 10_000_000, 23_000_000),
    });

    let changed = store.pop_dirty().unwrap();
    let next = engine.process_pool(changed, &store, 200);

    assert_eq!(next.len(), 1);
    assert!(next[0].expected_effective_profit > 0);
}

#[test]
fn graph_index_deduplicates_cycle_per_pool() {
    let graph = graph();
    assert_eq!(graph.pool_to_cycles[0].as_ref(), &[0]);
    assert_eq!(graph.pool_to_cycles[1].as_ref(), &[0]);
}
