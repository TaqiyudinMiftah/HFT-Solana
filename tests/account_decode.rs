use hft_solana::{
    decode::{
        pump::{decode_pool as decode_pump_pool, POOL_DISCRIMINATOR},
        raydium::{
            decode_amm_config, decode_pool_state, vault_amounts_without_fees,
            AMM_CONFIG_DISCRIMINATOR, POOL_STATE_DISCRIMINATOR,
        },
    },
    state::CreatorFeeOn,
};

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn pump_decoder_supports_appended_and_historical_fields() {
    let mut current = vec![0u8; 271];
    current[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    current[243] = 1;
    current[244] = 1;
    current[245..261].copy_from_slice(&123i128.to_le_bytes());
    put_u64(&mut current, 261, 77);
    current[269] = 1;
    current[270] = 1;

    let p = decode_pump_pool(&current).unwrap();
    assert!(p.is_mayhem_mode);
    assert!(p.is_cashback_coin);
    assert_eq!(p.virtual_quote_reserves, 123);
    assert_eq!(p.creator_fee_bps, 77);
    assert!(p.can_edit_creator_fee);
    assert!(p.is_holder_reward);

    let mut historical = current[..261].to_vec();
    historical[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    let old = decode_pump_pool(&historical).unwrap();
    assert_eq!(old.creator_fee_bps, 0);
    assert!(!old.can_edit_creator_fee);
    assert!(!old.is_holder_reward);
}

#[test]
fn raydium_decoder_reconstructs_fee_excluded_reserves() {
    let mut pool = vec![0u8; 413];
    pool[..8].copy_from_slice(&POOL_STATE_DISCRIMINATOR);
    put_u64(&mut pool, 341, 10);
    put_u64(&mut pool, 349, 20);
    put_u64(&mut pool, 357, 30);
    put_u64(&mut pool, 365, 40);
    pool[389] = 2;
    pool[390] = 1;
    put_u64(&mut pool, 397, 50);
    put_u64(&mut pool, 405, 60);

    let p = decode_pool_state(&pool).unwrap();
    assert_eq!(p.creator_fee_on, CreatorFeeOn::OnlyB);
    assert!(p.enable_creator_fee);
    assert_eq!(
        vault_amounts_without_fees(&p, 1_000, 2_000).unwrap(),
        (910, 1_880)
    );

    let mut config = vec![0u8; 116];
    config[..8].copy_from_slice(&AMM_CONFIG_DISCRIMINATOR);
    put_u64(&mut config, 12, 2_500);
    put_u64(&mut config, 20, 120_000);
    put_u64(&mut config, 28, 40_000);
    put_u64(&mut config, 108, 1_000);

    let c = decode_amm_config(&config).unwrap();
    assert_eq!(c.trade_fee_rate, 2_500);
    assert_eq!(c.protocol_fee_rate, 120_000);
    assert_eq!(c.fund_fee_rate, 40_000);
    assert_eq!(c.creator_fee_rate, 1_000);
}
