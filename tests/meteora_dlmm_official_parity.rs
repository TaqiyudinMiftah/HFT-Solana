#![cfg(feature = "meteora-dlmm")]

use std::{collections::HashMap, str::FromStr};

use hft_solana::{
    decode::meteora_dlmm::{decode_bin_array, decode_lb_pair},
    quote::meteora_dlmm::{
        discover_bin_array_pubkeys, missing_bin_array_pubkeys, quote_exact_in_official,
        MeteoraDlmmQuoteState,
    },
    types::{Direction, StateVersion},
};
use meteora_dlmm_commons::quote::quote_exact_in;
use solana_sdk_v2::{account::Account, clock::Clock, pubkey::Pubkey};

const LB_PAIR: &str = "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd";
const BIN_ARRAY_1: &str = "338HBraHxVupeftangX6jySecbND4osxcJjjMSW7qmMs";
const BIN_ARRAY_2: &str = "28BX6QycwTKx3CqswpJQs7hJCmoUs469Qt4maKMdhgmQ";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

fn version() -> StateVersion {
    StateVersion {
        slot: 356_410_171,
        write_version: 1,
        generation: 1,
    }
}

fn mint_account(data: &[u8]) -> Account {
    Account {
        lamports: 1,
        data: data.to_vec(),
        owner: Pubkey::from_str(TOKEN_PROGRAM).unwrap(),
        executable: false,
        rent_epoch: 0,
    }
}

fn fixture_state() -> MeteoraDlmmQuoteState {
    let lb_pair_data = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/lb_pair.bin"
    ));
    let bin_array_1_data = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/bin_array_1.bin"
    ));
    let bin_array_2_data = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/bin_array_2.bin"
    ));
    let mint_x_data = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/token_x_mint.bin"
    ));
    let mint_y_data = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/token_y_mint.bin"
    ));

    let lb_pair = decode_lb_pair(lb_pair_data).unwrap();
    let mut bin_arrays = HashMap::new();
    bin_arrays.insert(
        Pubkey::from_str(BIN_ARRAY_1).unwrap(),
        decode_bin_array(bin_array_1_data).unwrap(),
    );
    bin_arrays.insert(
        Pubkey::from_str(BIN_ARRAY_2).unwrap(),
        decode_bin_array(bin_array_2_data).unwrap(),
    );

    MeteoraDlmmQuoteState {
        lb_pair_pubkey: Pubkey::from_str(LB_PAIR).unwrap(),
        lb_pair,
        bin_arrays,
        bitmap_extension: None,
        mint_x_account: mint_account(mint_x_data),
        mint_y_account: mint_account(mint_y_data),
    }
}

#[test]
fn official_fixture_decodes_and_quotes_x_to_y() {
    let state = fixture_state();
    let amount_in = 40_000_000_000u64;
    let timestamp = 1_753_751_761u64;
    let slot = state.lb_pair.activation_point.saturating_add(1);
    let epoch = 0u64;

    let clock = Clock {
        slot,
        epoch,
        unix_timestamp: timestamp as i64,
        ..Clock::default()
    };

    let direct = quote_exact_in(
        state.lb_pair_pubkey,
        &state.lb_pair,
        amount_in,
        true,
        state.bin_arrays.clone(),
        None,
        &clock,
        &state.mint_x_account,
        &state.mint_y_account,
    )
    .unwrap();

    let wrapped = quote_exact_in_official(
        &state,
        amount_in,
        Direction::AtoB,
        timestamp,
        slot,
        epoch,
        version(),
    )
    .unwrap();

    assert!(direct.amount_out > 0);
    assert_eq!(wrapped.amount_out, direct.amount_out);
    assert_eq!(wrapped.dex_fee, direct.fee);
}

#[test]
fn official_fixture_decodes_and_quotes_y_to_x() {
    let state = fixture_state();
    let amount_in = 45_000_000_000_000u64;
    let timestamp = 1_753_751_761u64;
    let slot = state.lb_pair.activation_point.saturating_add(1);
    let epoch = 0u64;

    let clock = Clock {
        slot,
        epoch,
        unix_timestamp: timestamp as i64,
        ..Clock::default()
    };

    let direct = quote_exact_in(
        state.lb_pair_pubkey,
        &state.lb_pair,
        amount_in,
        false,
        state.bin_arrays.clone(),
        None,
        &clock,
        &state.mint_x_account,
        &state.mint_y_account,
    )
    .unwrap();

    let wrapped = quote_exact_in_official(
        &state,
        amount_in,
        Direction::BtoA,
        timestamp,
        slot,
        epoch,
        version(),
    )
    .unwrap();

    assert!(direct.amount_out > 0);
    assert_eq!(wrapped.amount_out, direct.amount_out);
    assert_eq!(wrapped.dex_fee, direct.fee);
}

#[test]
fn zero_input_is_rejected_before_official_quote() {
    let state = fixture_state();
    assert!(quote_exact_in_official(
        &state,
        0,
        Direction::AtoB,
        1_753_751_761,
        state.lb_pair.activation_point.saturating_add(1),
        0,
        version(),
    )
    .is_err());
}


#[test]
fn discovers_official_bin_array_window() {
    let state = fixture_state();

    let required = discover_bin_array_pubkeys(&state, Direction::AtoB, 2).unwrap();
    assert_eq!(
        required,
        vec![
            Pubkey::from_str(BIN_ARRAY_2).unwrap(),
            Pubkey::from_str(BIN_ARRAY_1).unwrap(),
        ]
    );
    assert!(missing_bin_array_pubkeys(&state, Direction::AtoB, 2)
        .unwrap()
        .is_empty());

    let mut incomplete = state.clone();
    incomplete
        .bin_arrays
        .remove(&Pubkey::from_str(BIN_ARRAY_1).unwrap());

    assert_eq!(
        missing_bin_array_pubkeys(&incomplete, Direction::AtoB, 2).unwrap(),
        vec![Pubkey::from_str(BIN_ARRAY_1).unwrap()]
    );
}
