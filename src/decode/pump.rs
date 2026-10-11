use crate::{
    decode::{check_discriminator, read_i128, read_pubkey, read_u64, DecodeError},
    quote::pump::PumpFeesBps,
    state::PumpState,
    types::StateVersion,
};

pub const POOL_DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PumpPoolAccount {
    pub creator: [u8; 32],
    pub base_mint: [u8; 32],
    pub quote_mint: [u8; 32],
    pub pool_base_token_account: [u8; 32],
    pub pool_quote_token_account: [u8; 32],
    pub coin_creator: [u8; 32],
    pub is_mayhem_mode: bool,
    pub is_cashback_coin: bool,
    pub virtual_quote_reserves: i128,
    pub creator_fee_bps: u64,
    pub can_edit_creator_fee: bool,
    pub is_holder_reward: bool,
}

/// Decode the stable prefix plus all currently appended PumpSwap Pool fields.
///
/// Historical accounts are supported: appended fields missing from shorter
/// layouts decode to their documented zero/false defaults.
pub fn decode_pool(data: &[u8]) -> Result<PumpPoolAccount, DecodeError> {
    // Historical layout through virtual_quote_reserves is 261 bytes including
    // the Anchor discriminator.
    if data.len() < 261 {
        return Err(DecodeError::TooShort);
    }
    check_discriminator(data, POOL_DISCRIMINATOR)?;

    let creator_fee_bps = if data.len() >= 269 {
        read_u64(data, 261)?
    } else {
        0
    };

    let can_edit_creator_fee = data.get(269).copied().unwrap_or(0) != 0;
    let is_holder_reward = data.get(270).copied().unwrap_or(0) != 0;

    Ok(PumpPoolAccount {
        creator: read_pubkey(data, 11)?,
        base_mint: read_pubkey(data, 43)?,
        quote_mint: read_pubkey(data, 75)?,
        pool_base_token_account: read_pubkey(data, 139)?,
        pool_quote_token_account: read_pubkey(data, 171)?,
        coin_creator: read_pubkey(data, 211)?,
        is_mayhem_mode: data[243] != 0,
        is_cashback_coin: data[244] != 0,
        virtual_quote_reserves: read_i128(data, 245)?,
        creator_fee_bps,
        can_edit_creator_fee,
        is_holder_reward,
    })
}

pub fn build_quote_state(
    pool: &PumpPoolAccount,
    base_vault_amount: u64,
    quote_vault_amount: u64,
    resolved_fees: PumpFeesBps,
    version: StateVersion,
) -> PumpState {
    PumpState {
        version,
        base_reserve: base_vault_amount,
        raw_quote_reserve: quote_vault_amount,
        virtual_quote_reserves: pool.virtual_quote_reserves,
        fees: resolved_fees,
        cashback_coin: pool.is_cashback_coin,
    }
}
