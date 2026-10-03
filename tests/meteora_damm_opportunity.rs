#![cfg(feature = "meteora-damm")]

use std::sync::Arc;

use hft_solana::{
    active_store::ActivePoolStore,
    graph::{Cycle, Edge, GraphIndex},
    opportunity_engine::{CycleSearchConfig, OpportunityEngine},
    reactor::ReactorOutput,
    state::{MeteoraDammState, PoolState},
    types::{Direction, StateVersion},
};
use meteora_cp_amm::{
    get_initial_pool_information,
    state::{CollectFeeMode, Pool},
    InitialPoolInformation,
};
use meteora_damm_sdk::calculate_initial_sqrt_price::calculate_compounding_initial_sqrt_price_and_liquidity;

fn version(generation: u64) -> StateVersion {
    StateVersion {
        slot: 500,
        write_version: generation,
        generation,
    }
}

fn compounding_pool(token_a_amount: u64, token_b_amount: u64) -> Pool {
    let (sqrt_price, liquidity) =
        calculate_compounding_initial_sqrt_price_and_liquidity(token_a_amount, token_b_amount)
            .unwrap();

    let InitialPoolInformation {
        token_a_amount,
        token_b_amount,
        sqrt_price,
        ..
    } = get_initial_pool_information(CollectFeeMode::Compounding, 0, 0, sqrt_price, liquidity)
        .unwrap();

    Pool {
        collect_fee_mode: CollectFeeMode::Compounding.into(),
        token_a_amount,
        token_b_amount,
        liquidity,
        sqrt_price,
        layout_version: 1,
        ..Default::default()
    }
}

fn state(pool: Pool, generation: u64) -> PoolState {
    PoolState::MeteoraDamm(MeteoraDammState {
        version: version(generation),
        pool: Arc::new(pool),
    })
}

#[test]
fn meteora_cycle_reaches_opportunity_engine_with_quote_context() {
    let graph = GraphIndex::from_cycles(
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
    );

    let mut engine = OpportunityEngine::new(
        graph,
        vec![CycleSearchConfig {
            initial_seed: 100_000,
            minimum_probe: 10_000,
            max_size: 1_000_000,
            minimum_effective_profit: 1,
            max_slot_skew: 0,
            expected_cu: 180_000,
        }],
    );

    let store = ActivePoolStore::new(2, 2);

    // Pool 0 values A at roughly 2 B, while pool 1 is approximately 1:1.
    // A -> B on pool 0 then B -> A on pool 1 is therefore profitable before
    // any landing cost is applied.
    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 0,
        state: state(compounding_pool(1_000_000_000, 2_000_000_000), 1),
    });
    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 1,
        state: state(compounding_pool(1_000_000_000, 1_000_000_000), 1),
    });

    let opportunities = engine.process_pool(0, &store, 1_753_751_761_000_000_000);

    assert_eq!(opportunities.len(), 1);
    let opportunity = &opportunities[0];
    assert_eq!(opportunity.cycle_id, 0);
    assert!(opportunity.amount_in >= 10_000);
    assert!(opportunity.amount_in <= 1_000_000);
    assert!(opportunity.expected_out > opportunity.amount_in);
    assert!(opportunity.expected_effective_profit > 0);
    assert_eq!(opportunity.expected_cu, 180_000);

    // The same state vector is deduplicated.
    assert!(engine
        .process_pool(1, &store, 1_753_751_761_100_000_000)
        .is_empty());
}
