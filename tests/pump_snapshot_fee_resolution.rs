use hft_solana::{
    decode::{
        pump::POOL_DISCRIMINATOR,
        pump_fee::{FEE_CONFIG_DISCRIMINATOR, FEE_CONFIG_SIZE_PRE_STABLE},
    },
    quote::pump::PumpFeeSchedule,
    snapshot::assemble_pump_state_from_config,
    types::StateVersion,
};

fn put_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn put_u128(data: &mut [u8], offset: usize, value: u128) {
    data[offset..offset + 16].copy_from_slice(&value.to_le_bytes());
}

fn token_account(mint: [u8; 32], amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; 165];
    data[..32].copy_from_slice(&mint);
    put_u64(&mut data, 64, amount);
    data[108] = 1;
    data
}

fn legacy_mint(supply: u64, decimals: u8) -> Vec<u8> {
    let mut data = vec![0u8; 82];
    data[..4].copy_from_slice(&0u32.to_le_bytes());
    put_u64(&mut data, 36, supply);
    data[44] = decimals;
    data[45] = 1;
    data[46..50].copy_from_slice(&0u32.to_le_bytes());
    data
}

fn fee_config() -> Vec<u8> {
    let mut data = vec![0u8; FEE_CONFIG_SIZE_PRE_STABLE];
    data[..8].copy_from_slice(&FEE_CONFIG_DISCRIMINATOR);

    let mut cursor = 41;
    // flat
    put_u64(&mut data, cursor, 10);
    put_u64(&mut data, cursor + 8, 20);
    put_u64(&mut data, cursor + 16, 30);
    cursor += 24;

    put_u32(&mut data, cursor, 2);
    cursor += 4;

    put_u128(&mut data, cursor, 0);
    cursor += 16;
    put_u64(&mut data, cursor, 1);
    put_u64(&mut data, cursor + 8, 2);
    put_u64(&mut data, cursor + 16, 3);
    cursor += 24;

    put_u128(&mut data, cursor, 2_000_000);
    cursor += 16;
    put_u64(&mut data, cursor, 4);
    put_u64(&mut data, cursor + 8, 5);
    put_u64(&mut data, cursor + 16, 6);

    data
}

#[test]
fn raw_accounts_resolve_market_cap_and_pool_creator_override() {
    let base_mint = [7u8; 32];
    let quote_mint = [9u8; 32];

    let mut pool = vec![0u8; 271];
    pool[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    pool[43..75].copy_from_slice(&base_mint);
    pool[75..107].copy_from_slice(&quote_mint);
    pool[211..243].copy_from_slice(&[8u8; 32]); // coin creator exists
    pool[245..261].copy_from_slice(&1_000i128.to_le_bytes());
    put_u64(&mut pool, 261, 99); // configurable creator fee

    let (state, meta) = assemble_pump_state_from_config(
        &pool,
        &fee_config(),
        &token_account(base_mint, 10_000),
        &token_account(quote_mint, 20_000),
        &legacy_mint(1_000_000, 6),
        &legacy_mint(1_000_000_000, 9),
        PumpFeeSchedule::CanonicalSolLike,
        true,
        StateVersion {
            slot: 10,
            write_version: 20,
            generation: 30,
        },
    )
    .unwrap();

    // (20_000 + 1_000) * 1_000_000 / 10_000
    assert_eq!(meta.market_cap, 2_100_000);

    // High tier gives 4/5/6, then pool-specific creator fee overrides 6 -> 99.
    assert_eq!(meta.resolved_fees.lp_fee_bps, 4);
    assert_eq!(meta.resolved_fees.protocol_fee_bps, 5);
    assert_eq!(meta.resolved_fees.creator_fee_bps, 99);

    assert_eq!(state.fees, meta.resolved_fees);
    assert_eq!(state.base_reserve, 10_000);
    assert_eq!(state.raw_quote_reserve, 20_000);
}
