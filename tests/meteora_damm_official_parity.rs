#![cfg(feature = "meteora-damm")]

use std::sync::Arc;

use hft_solana::{
    decode::meteora_damm::{
        decode_official_pool, decode_pool, POOL_ACCOUNT_LEN, POOL_DISCRIMINATOR,
    },
    graph::{Cycle, Edge},
    quote::meteora_damm::quote_exact_in_official,
    search::{quote_cycle_at, QuoteContext},
    state::{MeteoraDammState, PoolCell, PoolState},
    types::{Direction, StateVersion},
};
use meteora_cp_amm::{
    get_initial_pool_information,
    state::{CollectFeeMode, Pool},
    InitialPoolInformation,
};
use meteora_damm_sdk::calculate_initial_sqrt_price::calculate_compounding_initial_sqrt_price_and_liquidity;

fn version() -> StateVersion {
    StateVersion {
        slot: 356_410_171,
        write_version: 1,
        generation: 1,
    }
}

fn compounding_pool(token_a_amount: u64, token_b_amount: u64) -> Pool {
    let (sqrt_price, liquidity) =
        calculate_compounding_initial_sqrt_price_and_liquidity(token_a_amount, token_b_amount)
            .expect("valid compounding initialization");

    let InitialPoolInformation {
        token_a_amount,
        token_b_amount,
        sqrt_price,
        ..
    } = get_initial_pool_information(CollectFeeMode::Compounding, 0, 0, sqrt_price, liquidity)
        .expect("valid initial pool information");

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

fn pool_account_bytes(pool: &Pool) -> Vec<u8> {
    let mut data = vec![0u8; POOL_ACCOUNT_LEN];
    data[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    data[8..].copy_from_slice(bytemuck::bytes_of(pool));
    data
}

#[test]
fn manual_decoder_matches_official_pool_bytes() {
    let mut pool = compounding_pool(1_000_000_000, 2_000_000_000);
    pool.protocol_a_fee = 101;
    pool.protocol_b_fee = 202;
    pool.token_a_flag = 7;
    pool.token_b_flag = 8;
    pool.fee_version = 1;
    pool.creator = [9u8; 32].into();

    let data = pool_account_bytes(&pool);
    let manual = decode_pool(&data).unwrap();
    let official = decode_official_pool(&data).unwrap();

    assert_eq!(manual.liquidity, official.liquidity);
    assert_eq!(manual.sqrt_price, official.sqrt_price);
    assert_eq!(manual.collect_fee_mode, official.collect_fee_mode);
    assert_eq!(manual.protocol_a_fee, official.protocol_a_fee);
    assert_eq!(manual.protocol_b_fee, official.protocol_b_fee);
    assert_eq!(manual.token_a_amount, official.token_a_amount);
    assert_eq!(manual.token_b_amount, official.token_b_amount);
    assert_eq!(manual.layout_version, official.layout_version);
    assert_eq!(manual.fee_version, official.fee_version);
    assert_eq!(manual.creator, official.creator.to_bytes());
}

#[test]
fn wrapper_matches_official_sdk_for_both_directions() {
    let pool = compounding_pool(1_000_000_000, 2_000_000_000);
    let timestamp = 1_753_751_761;
    let slot = 356_410_171;
    let amount_in = 100_000;

    for (direction, a_to_b) in [(Direction::AtoB, true), (Direction::BtoA, false)] {
        let direct = meteora_damm_sdk::quote_exact_in::get_quote(
            &pool, timestamp, slot, amount_in, a_to_b, false,
        )
        .unwrap();

        let wrapped =
            quote_exact_in_official(&pool, amount_in, direction, timestamp, slot, version())
                .unwrap();

        let expected_fee = direct
            .claiming_fee
            .checked_add(direct.compounding_fee)
            .and_then(|value| value.checked_add(direct.protocol_fee))
            .and_then(|value| value.checked_add(direct.referral_fee))
            .unwrap();

        assert_eq!(wrapped.amount_in, amount_in);
        assert_eq!(wrapped.amount_out, direct.output_amount);
        assert_eq!(wrapped.dex_fee, expected_fee);
        assert_eq!(wrapped.version, version());
    }
}

#[test]
fn route_quote_requires_context_and_uses_official_damm_quote() {
    let pool = compounding_pool(1_000_000_000, 2_000_000_000);
    let state = PoolState::MeteoraDamm(MeteoraDammState {
        version: version(),
        pool: Arc::new(pool),
    });
    let pools = vec![PoolCell::new(state)];

    let cycle = Cycle {
        id: 0,
        len: 1,
        edges: [
            Edge {
                pool: 0,
                direction: Direction::AtoB,
                from_token: 0,
                to_token: 1,
            },
            Edge {
                pool: 0,
                direction: Direction::AtoB,
                from_token: 0,
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
    };

    assert!(hft_solana::search::quote_cycle(&cycle, &pools, 100_000).is_err());

    let context = QuoteContext {
        current_timestamp: 1_753_751_761,
        current_slot: 356_410_171,
    };
    let route = quote_cycle_at(&cycle, &pools, 100_000, context).unwrap();
    // The route-level assertion is intentionally simple: the wrapper used by
    // search returns a nonzero exact-in result when quote context is present.
    assert!(route.amount_out > 0);
}
