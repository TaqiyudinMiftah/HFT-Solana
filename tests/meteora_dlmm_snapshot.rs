#![cfg(feature = "meteora-dlmm")]

use std::str::FromStr;

use hft_solana::{
    quote::meteora_dlmm::quote_exact_in_official,
    snapshot::{
        assemble_meteora_dlmm_quote_state, validate_meteora_dlmm_bin_window, SnapshotError,
    },
    types::{Direction, StateVersion},
};
use solana_sdk_v2::pubkey::Pubkey;

const LB_PAIR: &str = "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd";
const BIN_ARRAY_1: &str = "338HBraHxVupeftangX6jySecbND4osxcJjjMSW7qmMs";
const BIN_ARRAY_2: &str = "28BX6QycwTKx3CqswpJQs7hJCmoUs469Qt4maKMdhgmQ";
const TOKEN_X_MINT: &str = "BBZU4HYvY4qMGE5MbWsVxGweGBZJqGRsgH8tAEAKusNk";
const TOKEN_Y_MINT: &str = "31iVdsS8fkURXg737XQwYhXVAuGv2vNYHjyDiURStkaU";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

fn version() -> StateVersion {
    StateVersion {
        slot: 356_410_171,
        write_version: 1,
        generation: 1,
    }
}

#[test]
fn raw_fixture_accounts_assemble_quote_ready_dlmm_state() {
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

    let pair_key = Pubkey::from_str(LB_PAIR).unwrap();
    let bin_1_key = Pubkey::from_str(BIN_ARRAY_1).unwrap();
    let bin_2_key = Pubkey::from_str(BIN_ARRAY_2).unwrap();
    let mint_x_key = Pubkey::from_str(TOKEN_X_MINT).unwrap();
    let mint_y_key = Pubkey::from_str(TOKEN_Y_MINT).unwrap();
    let token_program = Pubkey::from_str(TOKEN_PROGRAM).unwrap();

    let state = assemble_meteora_dlmm_quote_state(
        pair_key.to_bytes(),
        pair,
        &[
            (bin_1_key.to_bytes(), bin_1.as_slice()),
            (bin_2_key.to_bytes(), bin_2.as_slice()),
        ],
        None,
        mint_x_key.to_bytes(),
        token_program.to_bytes(),
        mint_x,
        mint_y_key.to_bytes(),
        token_program.to_bytes(),
        mint_y,
    )
    .unwrap();

    assert_eq!(state.lb_pair_pubkey, pair_key);
    assert_eq!(state.lb_pair.token_x_mint, mint_x_key);
    assert_eq!(state.lb_pair.token_y_mint, mint_y_key);
    assert_eq!(state.bin_arrays.len(), 2);

    let quote = quote_exact_in_official(
        &state,
        40_000_000_000,
        Direction::AtoB,
        1_753_751_761,
        state.lb_pair.activation_point.saturating_add(1),
        0,
        version(),
    )
    .unwrap();

    assert!(quote.amount_out > 0);
    assert!(quote.dex_fee > 0);
}

#[test]
fn raw_snapshot_rejects_configured_mint_mismatch() {
    let pair = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/lb_pair.bin"
    ));
    let bin_1 = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/bin_array_1.bin"
    ));
    let mint_x = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/token_x_mint.bin"
    ));
    let mint_y = include_bytes!(concat!(
        "../fixtures/meteora_dlmm/",
        "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/token_y_mint.bin"
    ));

    let pair_key = Pubkey::from_str(LB_PAIR).unwrap();
    let bin_1_key = Pubkey::from_str(BIN_ARRAY_1).unwrap();
    let mint_y_key = Pubkey::from_str(TOKEN_Y_MINT).unwrap();
    let token_program = Pubkey::from_str(TOKEN_PROGRAM).unwrap();

    let result = assemble_meteora_dlmm_quote_state(
        pair_key.to_bytes(),
        pair,
        &[(bin_1_key.to_bytes(), bin_1.as_slice())],
        None,
        [99u8; 32],
        token_program.to_bytes(),
        mint_x,
        mint_y_key.to_bytes(),
        token_program.to_bytes(),
        mint_y,
    );

    assert!(matches!(result, Err(SnapshotError::DlmmMintMismatch)));
}

#[test]
fn incomplete_discovered_bin_window_is_rejected() {
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

    let pair_key = Pubkey::from_str(LB_PAIR).unwrap();
    let bin_1_key = Pubkey::from_str(BIN_ARRAY_1).unwrap();
    let bin_2_key = Pubkey::from_str(BIN_ARRAY_2).unwrap();
    let mint_x_key = Pubkey::from_str(TOKEN_X_MINT).unwrap();
    let mint_y_key = Pubkey::from_str(TOKEN_Y_MINT).unwrap();
    let token_program = Pubkey::from_str(TOKEN_PROGRAM).unwrap();

    let mut state = assemble_meteora_dlmm_quote_state(
        pair_key.to_bytes(),
        pair,
        &[
            (bin_1_key.to_bytes(), bin_1.as_slice()),
            (bin_2_key.to_bytes(), bin_2.as_slice()),
        ],
        None,
        mint_x_key.to_bytes(),
        token_program.to_bytes(),
        mint_x,
        mint_y_key.to_bytes(),
        token_program.to_bytes(),
        mint_y,
    )
    .unwrap();

    validate_meteora_dlmm_bin_window(&state, 2).unwrap();

    state.bin_arrays.remove(&bin_1_key);
    assert!(matches!(
        validate_meteora_dlmm_bin_window(&state, 2),
        Err(SnapshotError::DlmmIncompleteBinWindow {
            direction: Direction::AtoB,
            missing: 1,
        })
    ));
}
