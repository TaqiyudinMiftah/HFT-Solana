use hft_solana::decode::meteora_damm::{decode_pool, POOL_ACCOUNT_LEN, POOL_DISCRIMINATOR};

fn put_u16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn put_u128(data: &mut [u8], offset: usize, value: u128) {
    data[offset..offset + 16].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn decodes_current_official_zero_copy_pool_layout() {
    let mut data = vec![0u8; POOL_ACCOUNT_LEN];
    data[..8].copy_from_slice(&POOL_DISCRIMINATOR);

    data[8..40].copy_from_slice(&[0xabu8; 32]);
    data[48] = 25;
    data[50] = 15;
    put_u16(&mut data, 54, 4_000);

    data[56] = 1;
    put_u32(&mut data, 64, 350_000);
    put_u32(&mut data, 68, 12_345);
    put_u16(&mut data, 72, 10);
    put_u16(&mut data, 74, 30);
    put_u16(&mut data, 76, 120);
    put_u16(&mut data, 78, 5_000);
    put_u64(&mut data, 80, 1_700_000_000);
    put_u128(&mut data, 88, 111);
    put_u128(&mut data, 104, 222);
    put_u128(&mut data, 120, 333);
    put_u128(&mut data, 136, 444);
    put_u128(&mut data, 152, 555);

    data[168..200].copy_from_slice(&[1u8; 32]);
    data[200..232].copy_from_slice(&[2u8; 32]);
    data[232..264].copy_from_slice(&[3u8; 32]);
    data[264..296].copy_from_slice(&[4u8; 32]);
    data[296..328].copy_from_slice(&[5u8; 32]);

    put_u128(&mut data, 360, 1_000_000);
    put_u64(&mut data, 392, 101);
    put_u64(&mut data, 400, 202);
    put_u64(&mut data, 408, 303);
    put_u128(&mut data, 424, 10_000);
    put_u128(&mut data, 440, 90_000);
    put_u128(&mut data, 456, 40_000);
    put_u64(&mut data, 472, 999);
    data[480] = 1;
    data[481] = 0;
    data[482] = 7;
    data[483] = 8;
    data[484] = 2;
    data[485] = 1;
    data[486] = 1;
    put_u128(&mut data, 552, 123_456);
    data[648..680].copy_from_slice(&[9u8; 32]);
    put_u64(&mut data, 680, 12_000_000);
    put_u64(&mut data, 688, 34_000_000);
    data[696] = 1;

    let pool = decode_pool(&data).unwrap();

    assert_eq!(pool.fees.base_fee_info, [0xabu8; 32]);
    assert_eq!(pool.fees.protocol_fee_percent, 25);
    assert_eq!(pool.fees.referral_fee_percent, 15);
    assert_eq!(pool.fees.compounding_fee_bps, 4_000);
    assert!(pool.fees.dynamic_fee.initialized);
    assert_eq!(pool.fees.dynamic_fee.max_volatility_accumulator, 350_000);
    assert_eq!(pool.fees.dynamic_fee.variable_fee_control, 12_345);
    assert_eq!(pool.fees.dynamic_fee.bin_step, 10);
    assert_eq!(pool.fees.dynamic_fee.last_update_timestamp, 1_700_000_000);
    assert_eq!(pool.fees.dynamic_fee.sqrt_price_reference, 222);
    assert_eq!(pool.fees.dynamic_fee.volatility_accumulator, 333);
    assert_eq!(pool.fees.init_sqrt_price, 555);

    assert_eq!(pool.token_a_mint, [1u8; 32]);
    assert_eq!(pool.token_b_mint, [2u8; 32]);
    assert_eq!(pool.token_a_vault, [3u8; 32]);
    assert_eq!(pool.token_b_vault, [4u8; 32]);
    assert_eq!(pool.whitelisted_vault, [5u8; 32]);
    assert_eq!(pool.liquidity, 1_000_000);
    assert_eq!(pool.protocol_a_fee, 101);
    assert_eq!(pool.protocol_b_fee, 202);
    assert_eq!(pool.dead_liquidity_fee_checkpoint, 303);
    assert_eq!(pool.sqrt_min_price, 10_000);
    assert_eq!(pool.sqrt_max_price, 90_000);
    assert_eq!(pool.sqrt_price, 40_000);
    assert_eq!(pool.activation_point, 999);
    assert_eq!(pool.collect_fee_mode, 2);
    assert_eq!(pool.fee_version, 1);
    assert_eq!(pool.permanent_lock_liquidity, 123_456);
    assert_eq!(pool.creator, [9u8; 32]);
    assert_eq!(pool.token_a_amount, 12_000_000);
    assert_eq!(pool.token_b_amount, 34_000_000);
    assert_eq!(pool.layout_version, 1);
}

#[test]
fn rejects_truncated_pool_account() {
    let mut data = vec![0u8; POOL_ACCOUNT_LEN - 1];
    data[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    assert!(decode_pool(&data).is_err());
}
