use std::str::FromStr;

use hft_solana::{
    decode::{
        pump::PumpPoolAccount,
        pump_global::{
            decode_pump_amm_global_config, decode_pump_global, PumpGlobalAccount,
            PUMP_AMM_GLOBAL_CONFIG_DISCRIMINATOR, PUMP_GLOBAL_DISCRIMINATOR,
        },
    },
    quote::{
        pump::PumpFeeSchedule,
        pump_identity::{classify_fee_schedule, pump_pool_authority, WSOL_MINT},
    },
};
use solana_pubkey::Pubkey;

fn pool(base_mint: [u8; 32], quote_mint: [u8; 32], creator: [u8; 32]) -> PumpPoolAccount {
    PumpPoolAccount {
        creator,
        base_mint,
        quote_mint,
        pool_base_token_account: [0; 32],
        pool_quote_token_account: [0; 32],
        coin_creator: [9; 32],
        is_mayhem_mode: false,
        is_cashback_coin: false,
        virtual_quote_reserves: 0,
        creator_fee_bps: 0,
        can_edit_creator_fee: false,
        is_holder_reward: false,
    }
}

#[test]
fn classifies_all_pump_fee_schedules() {
    let base = [7u8; 32];
    let stable = [8u8; 32];
    let exotic = [9u8; 32];
    let canonical_creator = pump_pool_authority(base).to_bytes();

    let global = PumpGlobalAccount {
        whitelisted_quote_mints: [stable],
        creator_fee_configurable: true,
        max_configurable_creator_fee_bps: 300,
    };

    let wsol = Pubkey::from_str(WSOL_MINT).unwrap().to_bytes();

    assert_eq!(
        classify_fee_schedule(&pool(base, wsol, canonical_creator), &global),
        PumpFeeSchedule::CanonicalSolLike
    );
    assert_eq!(
        classify_fee_schedule(&pool(base, stable, canonical_creator), &global),
        PumpFeeSchedule::CanonicalStable
    );
    assert_eq!(
        classify_fee_schedule(&pool(base, exotic, canonical_creator), &global),
        PumpFeeSchedule::CanonicalExotic
    );
    assert_eq!(
        classify_fee_schedule(&pool(base, wsol, [1u8; 32]), &global),
        PumpFeeSchedule::NonCanonical
    );
}

#[test]
fn decodes_current_global_offsets() {
    let stable = [4u8; 32];

    let mut pump_global = vec![0u8; 1_087];
    pump_global[..8].copy_from_slice(&PUMP_GLOBAL_DISCRIMINATOR);
    pump_global[1_013..1_045].copy_from_slice(&stable);
    pump_global[1_045] = 1;
    pump_global[1_046..1_054].copy_from_slice(&250u64.to_le_bytes());

    let decoded = decode_pump_global(&pump_global).unwrap();
    assert_eq!(decoded.whitelisted_quote_mints, [stable]);
    assert!(decoded.creator_fee_configurable);
    assert_eq!(decoded.max_configurable_creator_fee_bps, 250);

    let mut amm_global = vec![0u8; 949];
    amm_global[..8].copy_from_slice(&PUMP_AMM_GLOBAL_CONFIG_DISCRIMINATOR);
    amm_global[940] = 1;
    amm_global[941..949].copy_from_slice(&175u64.to_le_bytes());

    let decoded = decode_pump_amm_global_config(&amm_global).unwrap();
    assert!(decoded.creator_fee_configurable);
    assert_eq!(decoded.max_configurable_creator_fee_bps, 175);
}
