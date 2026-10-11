#![cfg(feature = "meteora-dlmm")]

use std::str::FromStr;
use std::sync::Arc;

use hft_solana::{
    active_store::ActivePoolStore,
    graph::{Cycle, Edge, GraphIndex},
    opportunity_engine::{CycleSearchConfig, OpportunityEngine},
    quote::raydium::RaydiumFees,
    reactor::ReactorOutput,
    snapshot::assemble_meteora_dlmm_quote_state,
    state::{CreatorFeeOn, MeteoraDlmmState, PoolState, RaydiumCpmmState},
    types::{Direction, StateVersion},
};
use solana_sdk_v2::pubkey::Pubkey;

const LB_PAIR: &str = "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd";
const BIN_ARRAY_1: &str = "338HBraHxVupeftangX6jySecbND4osxcJjjMSW7qmMs";
const BIN_ARRAY_2: &str = "28BX6QycwTKx3CqswpJQs7hJCmoUs469Qt4maKMdhgmQ";
const TOKEN_X_MINT: &str = "BBZU4HYvY4qMGE5MbWsVxGweGBZJqGRsgH8tAEAKusNk";
const TOKEN_Y_MINT: &str = "31iVdsS8fkURXg737XQwYhXVAuGv2vNYHjyDiURStkaU";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

fn key(value: &str) -> [u8; 32] {
    Pubkey::from_str(value).unwrap().to_bytes()
}

fn fixture_state() -> hft_solana::quote::meteora_dlmm::MeteoraDlmmQuoteState {
    let pair = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/lb_pair.bin"
    ));
    let bin_1 = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/bin_array_1.bin"
    ));
    let bin_2 = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/bin_array_2.bin"
    ));
    let mint_x = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/token_x_mint.bin"
    ));
    let mint_y = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/token_y_mint.bin"
    ));

    assemble_meteora_dlmm_quote_state(
        key(LB_PAIR),
        pair,
        &[
            (key(BIN_ARRAY_1), bin_1.as_slice()),
            (key(BIN_ARRAY_2), bin_2.as_slice()),
        ],
        None,
        key(TOKEN_X_MINT),
        key(TOKEN_PROGRAM),
        mint_x,
        key(TOKEN_Y_MINT),
        key(TOKEN_PROGRAM),
        mint_y,
    )
    .unwrap()
}

#[test]
fn dlmm_can_participate_in_profitable_mixed_cycle() {
    let quote_state = fixture_state();
    let slot = quote_state.lb_pair.activation_point.saturating_add(1);
    let version = StateVersion {
        slot,
        write_version: 1,
        generation: 1,
    };

    let dlmm = PoolState::MeteoraDlmm(MeteoraDlmmState {
        version,
        quote: Arc::new(quote_state),
    });

    // The synthetic second leg deliberately gives Y -> X a very favorable
    // rate. Its purpose is not market realism; it proves DLMM output is fed
    // through the same cycle sizing/opportunity machinery as other DEXes.
    let raydium = PoolState::RaydiumCpmm(RaydiumCpmmState {
        version,
        reserve_a: 18_000_000_000_000_000_000,
        reserve_b: 1_000_000_000_000,
        fees: RaydiumFees {
            trade_fee_rate: 0,
            creator_fee_rate: 0,
            protocol_fee_rate: 0,
            fund_fee_rate: 0,
        },
        creator_fee_on: CreatorFeeOn::Both,
    });

    let mut store = ActivePoolStore::new(2, 2);
    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 0,
        state: dlmm,
    });
    store.apply(ReactorOutput::PoolUpdated {
        pool_id: 1,
        state: raydium,
    });

    let graph = GraphIndex::from_cycles(
        2,
        vec![Cycle {
            id: 0,
            len: 2,
            start_token: 0,
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
        }],
    );

    let mut engine = OpportunityEngine::new(
        graph,
        vec![CycleSearchConfig {
            initial_seed: 40_000_000_000,
            minimum_probe: 40_000_000_000,
            max_size: 80_000_000_000,
            minimum_effective_profit: 1,
            max_slot_skew: 0,
            expected_cu: 220_000,
        }],
    );

    let opportunities = engine.process_pool(0, &store, 1_753_751_761_000_000_000);
    assert_eq!(opportunities.len(), 1);
    assert!(opportunities[0].expected_out > opportunities[0].amount_in);
    assert!(opportunities[0].expected_effective_profit > 0);
    assert_eq!(opportunities[0].expected_cu, 220_000);

    // Same state vector is deduplicated even if the second pool wakes search.
    assert!(engine
        .process_pool(1, &store, 1_753_751_761_100_000_000)
        .is_empty());
}
