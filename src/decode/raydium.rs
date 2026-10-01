use crate::{
    decode::{check_discriminator, read_pubkey, read_u64, DecodeError},
    quote::raydium::RaydiumFees,
    state::{CreatorFeeOn, RaydiumCpmmState},
    types::StateVersion,
};

pub const POOL_STATE_DISCRIMINATOR: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];
pub const AMM_CONFIG_DISCRIMINATOR: [u8; 8] = [218, 244, 33, 104, 203, 203, 43, 111];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaydiumPoolAccount {
    pub amm_config: [u8; 32],
    pub token_0_vault: [u8; 32],
    pub token_1_vault: [u8; 32],
    pub token_0_mint: [u8; 32],
    pub token_1_mint: [u8; 32],
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,
    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,
    pub creator_fee_on: CreatorFeeOn,
    pub enable_creator_fee: bool,
    pub creator_fees_token_0: u64,
    pub creator_fees_token_1: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaydiumAmmConfig {
    pub trade_fee_rate: u64,
    pub protocol_fee_rate: u64,
    pub fund_fee_rate: u64,
    pub creator_fee_rate: u64,
}

pub fn decode_pool_state(data: &[u8]) -> Result<RaydiumPoolAccount, DecodeError> {
    if data.len() < 413 {
        return Err(DecodeError::TooShort);
    }
    check_discriminator(data, POOL_STATE_DISCRIMINATOR)?;

    let creator_fee_on = match data[389] {
        0 => CreatorFeeOn::Both,
        1 => CreatorFeeOn::OnlyA,
        2 => CreatorFeeOn::OnlyB,
        _ => return Err(DecodeError::InvalidValue),
    };

    Ok(RaydiumPoolAccount {
        amm_config: read_pubkey(data, 8)?,
        token_0_vault: read_pubkey(data, 72)?,
        token_1_vault: read_pubkey(data, 104)?,
        token_0_mint: read_pubkey(data, 168)?,
        token_1_mint: read_pubkey(data, 200)?,
        protocol_fees_token_0: read_u64(data, 341)?,
        protocol_fees_token_1: read_u64(data, 349)?,
        fund_fees_token_0: read_u64(data, 357)?,
        fund_fees_token_1: read_u64(data, 365)?,
        creator_fee_on,
        enable_creator_fee: data[390] != 0,
        creator_fees_token_0: read_u64(data, 397)?,
        creator_fees_token_1: read_u64(data, 405)?,
    })
}

pub fn decode_amm_config(data: &[u8]) -> Result<RaydiumAmmConfig, DecodeError> {
    if data.len() < 116 {
        return Err(DecodeError::TooShort);
    }
    check_discriminator(data, AMM_CONFIG_DISCRIMINATOR)?;

    Ok(RaydiumAmmConfig {
        trade_fee_rate: read_u64(data, 12)?,
        protocol_fee_rate: read_u64(data, 20)?,
        fund_fee_rate: read_u64(data, 28)?,
        creator_fee_rate: read_u64(data, 108)?,
    })
}

pub fn vault_amounts_without_fees(
    pool: &RaydiumPoolAccount,
    vault_0_amount: u64,
    vault_1_amount: u64,
) -> Result<(u64, u64), DecodeError> {
    let fees_0 = pool
        .protocol_fees_token_0
        .checked_add(pool.fund_fees_token_0)
        .and_then(|x| x.checked_add(pool.creator_fees_token_0))
        .ok_or(DecodeError::Math)?;

    let fees_1 = pool
        .protocol_fees_token_1
        .checked_add(pool.fund_fees_token_1)
        .and_then(|x| x.checked_add(pool.creator_fees_token_1))
        .ok_or(DecodeError::Math)?;

    Ok((
        vault_0_amount.checked_sub(fees_0).ok_or(DecodeError::Math)?,
        vault_1_amount.checked_sub(fees_1).ok_or(DecodeError::Math)?,
    ))
}

pub fn build_quote_state(
    pool: &RaydiumPoolAccount,
    config: RaydiumAmmConfig,
    vault_0_amount: u64,
    vault_1_amount: u64,
    version: StateVersion,
) -> Result<RaydiumCpmmState, DecodeError> {
    let (reserve_a, reserve_b) =
        vault_amounts_without_fees(pool, vault_0_amount, vault_1_amount)?;

    Ok(RaydiumCpmmState {
        version,
        reserve_a,
        reserve_b,
        fees: RaydiumFees {
            trade_fee_rate: config.trade_fee_rate,
            creator_fee_rate: if pool.enable_creator_fee {
                config.creator_fee_rate
            } else {
                0
            },
            protocol_fee_rate: config.protocol_fee_rate,
            fund_fee_rate: config.fund_fee_rate,
        },
        creator_fee_on: pool.creator_fee_on,
    })
}
