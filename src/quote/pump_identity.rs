use std::str::FromStr;

use solana_pubkey::Pubkey;

use crate::{
    decode::{pump::PumpPoolAccount, pump_global::PumpGlobalAccount},
    quote::pump::PumpFeeSchedule,
};

pub const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

fn pubkey(bytes: [u8; 32]) -> Pubkey {
    Pubkey::new_from_array(bytes)
}

pub fn pump_pool_authority(base_mint: [u8; 32]) -> Pubkey {
    let program_id = Pubkey::from_str(PUMP_PROGRAM_ID).expect("static Pump program id");
    Pubkey::find_program_address(&[b"pool-authority", &base_mint], &program_id).0
}

pub fn is_canonical_pool(pool: &PumpPoolAccount) -> bool {
    pubkey(pool.creator) == pump_pool_authority(pool.base_mint)
}

pub fn classify_fee_schedule(
    pool: &PumpPoolAccount,
    pump_global: &PumpGlobalAccount,
) -> PumpFeeSchedule {
    if !is_canonical_pool(pool) {
        return PumpFeeSchedule::NonCanonical;
    }

    let quote = pubkey(pool.quote_mint);
    let wsol = Pubkey::from_str(WSOL_MINT).expect("static WSOL mint");

    if quote == wsol {
        return PumpFeeSchedule::CanonicalSolLike;
    }

    if pump_global
        .whitelisted_quote_mints
        .iter()
        .any(|mint| *mint != [0u8; 32] && *mint == pool.quote_mint)
    {
        return PumpFeeSchedule::CanonicalStable;
    }

    PumpFeeSchedule::CanonicalExotic
}
