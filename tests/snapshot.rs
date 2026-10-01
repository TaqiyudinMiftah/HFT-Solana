use hft_solana::{
    decode::{
        pump::POOL_DISCRIMINATOR,
        raydium::{AMM_CONFIG_DISCRIMINATOR, POOL_STATE_DISCRIMINATOR},
    },
    quote::pump::PumpFeesBps,
    snapshot::{
        assemble_pump_state, assemble_raydium_state, MintQuoteSafety, SnapshotError,
    },
    types::StateVersion,
};

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn token_account(mint: [u8; 32], amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; 165];
    data[..32].copy_from_slice(&mint);
    put_u64(&mut data, 64, amount);
    data[108] = 1;
    data
}

fn version() -> StateVersion {
    StateVersion {
        slot: 9,
        write_version: 7,
        generation: 3,
    }
}

#[test]
fn assembles_pump_snapshot_from_pool_and_vault_bytes() {
    let base_mint = [7u8; 32];
    let quote_mint = [9u8; 32];

    let mut pool = vec![0u8; 271];
    pool[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    pool[43..75].copy_from_slice(&base_mint);
    pool[75..107].copy_from_slice(&quote_mint);
    pool[244] = 1;
    pool[245..261].copy_from_slice(&1_000i128.to_le_bytes());

    let state = assemble_pump_state(
        &pool,
        &token_account(base_mint, 10_000),
        &token_account(quote_mint, 20_000),
        PumpFeesBps {
            lp_fee_bps: 20,
            protocol_fee_bps: 5,
            creator_fee_bps: 50,
        },
        MintQuoteSafety::Safe,
        MintQuoteSafety::Safe,
        version(),
    )
    .unwrap();

    assert_eq!(state.base_reserve, 10_000);
    assert_eq!(state.raw_quote_reserve, 20_000);
    assert_eq!(state.virtual_quote_reserves, 1_000);
    assert!(state.cashback_coin);
}

#[test]
fn rejects_unresolved_token_2022_behavior() {
    let base_mint = [7u8; 32];
    let quote_mint = [9u8; 32];

    let mut pool = vec![0u8; 271];
    pool[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    pool[43..75].copy_from_slice(&base_mint);
    pool[75..107].copy_from_slice(&quote_mint);

    let err = assemble_pump_state(
        &pool,
        &token_account(base_mint, 10_000),
        &token_account(quote_mint, 20_000),
        PumpFeesBps::default(),
        MintQuoteSafety::UnsupportedOrUnknown,
        MintQuoteSafety::Safe,
        version(),
    )
    .unwrap_err();

    assert_eq!(err, SnapshotError::UnsupportedMintBehavior);
}

#[test]
fn assembles_raydium_snapshot_and_removes_accrued_fees() {
    let mint_0 = [1u8; 32];
    let mint_1 = [2u8; 32];

    let mut pool = vec![0u8; 413];
    pool[..8].copy_from_slice(&POOL_STATE_DISCRIMINATOR);
    pool[168..200].copy_from_slice(&mint_0);
    pool[200..232].copy_from_slice(&mint_1);
    put_u64(&mut pool, 341, 10);
    put_u64(&mut pool, 349, 20);
    put_u64(&mut pool, 357, 30);
    put_u64(&mut pool, 365, 40);
    pool[389] = 0;
    pool[390] = 1;
    put_u64(&mut pool, 397, 50);
    put_u64(&mut pool, 405, 60);

    let mut config = vec![0u8; 116];
    config[..8].copy_from_slice(&AMM_CONFIG_DISCRIMINATOR);
    put_u64(&mut config, 12, 2_500);
    put_u64(&mut config, 20, 120_000);
    put_u64(&mut config, 28, 40_000);
    put_u64(&mut config, 108, 1_000);

    let state = assemble_raydium_state(
        &pool,
        &config,
        &token_account(mint_0, 1_000),
        &token_account(mint_1, 2_000),
        MintQuoteSafety::Safe,
        MintQuoteSafety::Safe,
        version(),
    )
    .unwrap();

    assert_eq!(state.reserve_a, 910);
    assert_eq!(state.reserve_b, 1_880);
    assert_eq!(state.fees.creator_fee_rate, 1_000);
}
