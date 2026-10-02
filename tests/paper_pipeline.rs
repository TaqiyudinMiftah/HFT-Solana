use hft_solana::{
    active_store::ActivePoolStore,
    graph::{Cycle, Edge, GraphIndex},
    opportunity_engine::{CycleSearchConfig, OpportunityEngine},
    paper::PaperPipeline,
    quote::{pump::PumpFeesBps, raydium::RaydiumFees},
    reactor::{PaperStateReactor, ReactorOutput},
    state::{CreatorFeeOn, PoolState, PumpState, RaydiumCpmmState},
    types::{Direction, StateVersion},
};

fn version(generation: u64) -> StateVersion {
    StateVersion {
        slot: 100,
        write_version: generation,
        generation,
    }
}

fn pump_state() -> PoolState {
    PoolState::Pump(PumpState {
        version: version(1),
        base_reserve: 1_000_000,
        raw_quote_reserve: 1_000_000,
        virtual_quote_reserves: 0,
        fees: PumpFeesBps::default(),
        cashback_coin: false,
    })
}

fn raydium_state() -> PoolState {
    raydium_state_with(1, 1_000_000, 1_100_000)
}

fn raydium_state_with(generation: u64, reserve_a: u64, reserve_b: u64) -> PoolState {
    PoolState::RaydiumCpmm(RaydiumCpmmState {
        version: version(generation),
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

fn cycle() -> Cycle {
    Cycle {
        id: 0,
        len: 2,
        start_token: 0,
        edges: [
            Edge {
                pool: 0,
                direction: Direction::BtoA,
                from_token: 0,
                to_token: 1,
            },
            Edge {
                pool: 1,
                direction: Direction::AtoB,
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
    }
}

fn engine() -> OpportunityEngine {
    let graph = GraphIndex::from_cycles(2, vec![cycle()]);
    OpportunityEngine::new(
        graph,
        vec![CycleSearchConfig {
            initial_seed: 1_000,
            minimum_probe: 1_000,
            max_size: 4_000,
            minimum_effective_profit: 1,
            max_slot_skew: 0,
            expected_cu: 120_000,
        }],
    )
}

#[test]
fn queue_full_fallback_preserves_latest_opportunity_without_duplicates() {
    // Capacity one deliberately forces the second pool update down the
    // synchronous overflow fallback path.
    let reactor = PaperStateReactor::new(2, 0);
    let store = ActivePoolStore::new(2, 1);
    let mut pipeline = PaperPipeline::new(reactor, store, engine());

    let batch = pipeline.process_outputs(
        vec![
            ReactorOutput::PoolUpdated {
                pool_id: 0,
                state: pump_state(),
            },
            ReactorOutput::PoolUpdated {
                pool_id: 1,
                state: raydium_state(),
            },
        ],
        123,
    );

    assert_eq!(batch.queue_full_fallbacks, 1);
    assert_eq!(batch.opportunities.len(), 1);
    assert_eq!(batch.opportunities[0].cycle_id, 0);
    assert!(batch.opportunities[0].expected_effective_profit > 0);

    // Pool 0 is still drained afterward, but the engine sees the same captured
    // generations and suppresses a duplicate opportunity.
    assert_eq!(pipeline.stats().opportunities_emitted, 1);
    assert_eq!(pipeline.stats().queue_full_fallbacks, 1);
    assert_eq!(pipeline.stats().pools_evaluated, 2);
}

#[test]
fn invalidation_marks_pool_unavailable_and_emits_no_trade() {
    let reactor = PaperStateReactor::new(2, 0);
    let store = ActivePoolStore::new(2, 2);
    let mut pipeline = PaperPipeline::new(reactor, store, engine());

    pipeline.process_outputs(
        vec![
            ReactorOutput::PoolUpdated {
                pool_id: 0,
                state: pump_state(),
            },
            ReactorOutput::PoolUpdated {
                pool_id: 1,
                state: raydium_state(),
            },
        ],
        1,
    );

    let batch = pipeline.process_outputs(
        vec![ReactorOutput::PoolInvalidated {
            pool_id: 1,
            reason: hft_solana::reactor::ReactorInvalidation::Snapshot("test".into()),
        }],
        2,
    );

    assert!(!pipeline.store().is_ready(1));
    assert!(batch.opportunities.is_empty());
}


fn three_pool_engine() -> OpportunityEngine {
    let graph = GraphIndex::from_cycles(
        3,
        vec![Cycle {
            id: 0,
            len: 2,
            start_token: 0,
            edges: [
                Edge {
                    pool: 1,
                    direction: Direction::AtoB,
                    from_token: 0,
                    to_token: 1,
                },
                Edge {
                    pool: 2,
                    direction: Direction::BtoA,
                    from_token: 1,
                    to_token: 0,
                },
                Edge {
                    pool: 1,
                    direction: Direction::AtoB,
                    from_token: 0,
                    to_token: 0,
                },
            ],
        }],
    );

    OpportunityEngine::new(
        graph,
        vec![CycleSearchConfig {
            initial_seed: 1_000,
            minimum_probe: 100,
            max_size: 10_000,
            minimum_effective_profit: 1,
            max_slot_skew: 0,
            expected_cu: 120_000,
        }],
    )
}

#[test]
fn queue_full_fallback_waits_until_entire_batch_is_published() {
    let reactor = PaperStateReactor::new(2, 0);
    let store = ActivePoolStore::new(3, 1);
    let mut pipeline = PaperPipeline::new(reactor, store, three_pool_engine());

    let initial = pipeline.process_outputs(
        vec![
            ReactorOutput::PoolUpdated {
                pool_id: 1,
                state: raydium_state_with(1, 10_000_000, 22_000_000),
            },
            ReactorOutput::PoolUpdated {
                pool_id: 2,
                state: raydium_state_with(1, 10_000_000, 20_000_000),
            },
        ],
        10,
    );
    assert_eq!(initial.opportunities.len(), 1);

    let batch = pipeline.process_outputs(
        vec![
            ReactorOutput::PoolUpdated {
                pool_id: 0,
                state: pump_state(),
            },
            ReactorOutput::PoolUpdated {
                pool_id: 1,
                state: raydium_state_with(2, 10_000_000, 23_000_000),
            },
            ReactorOutput::PoolUpdated {
                pool_id: 2,
                state: raydium_state_with(2, 10_000_000, 19_000_000),
            },
        ],
        20,
    );

    // Immediate overflow search would evaluate pool 1 before pool 2's update
    // was published, then evaluate the final state again. The fixed pipeline
    // publishes the full batch first and emits only the final state vector.
    assert_eq!(batch.queue_full_fallbacks, 2);
    assert_eq!(batch.opportunities.len(), 1);
    assert!(batch.opportunities[0].expected_effective_profit > 0);
}
