use thiserror::Error;

use crate::{
    decode::{
        mint::{inspect_mint, MintInspectError, MintQuoteInfo},
        pump::{build_quote_state as build_pump_quote_state, decode_pool as decode_pump_pool},
        pump_fee::decode_fee_config,
        raydium::{
            build_quote_state as build_raydium_quote_state, decode_amm_config,
            decode_pool_state,
        },
        token::{decode_token_account_base, TokenAccountState},
        DecodeError,
    },
    quote::{
        pump::{
            effective_quote_reserve, pool_market_cap, resolve_pool_fees, PumpFeeSchedule,
            PumpFeesBps,
        },
        QuoteError,
    },
    state::{PumpState, RaydiumCpmmState},
    types::StateVersion,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MintQuoteSafety {
    /// Legacy SPL Token mint or Token-2022 mint whose extensions have been
    /// decoded and confirmed not to affect quote economics.
    Safe,
    /// Transfer fee, transfer hook, or another quote-affecting behavior exists
    /// or has not yet been resolved.
    UnsupportedOrUnknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PumpSnapshotMeta {
    pub base_mint: MintQuoteInfo,
    pub quote_mint: MintQuoteInfo,
    pub market_cap: u128,
    pub resolved_fees: PumpFeesBps,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SnapshotError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Quote(#[from] QuoteError),
    #[error("invalid mint account")]
    MintDecode,
    #[error("vault mint does not match pool mint")]
    VaultMintMismatch,
    #[error("vault token account is not initialized")]
    VaultNotInitialized,
    #[error("mint has unsupported or unresolved quote-affecting extensions")]
    UnsupportedMintBehavior,
}

impl From<MintInspectError> for SnapshotError {
    fn from(_: MintInspectError) -> Self {
        SnapshotError::MintDecode
    }
}

#[inline]
fn require_quote_safe(safety: MintQuoteSafety) -> Result<(), SnapshotError> {
    match safety {
        MintQuoteSafety::Safe => Ok(()),
        MintQuoteSafety::UnsupportedOrUnknown => Err(SnapshotError::UnsupportedMintBehavior),
    }
}

#[inline]
fn require_initialized(state: TokenAccountState) -> Result<(), SnapshotError> {
    if state == TokenAccountState::Initialized {
        Ok(())
    } else {
        Err(SnapshotError::VaultNotInitialized)
    }
}

pub fn inspect_pair_mints(
    mint_a_data: &[u8],
    mint_b_data: &[u8],
) -> Result<(MintQuoteInfo, MintQuoteInfo), SnapshotError> {
    Ok((inspect_mint(mint_a_data)?, inspect_mint(mint_b_data)?))
}

pub fn assemble_pump_state(
    pool_data: &[u8],
    base_vault_data: &[u8],
    quote_vault_data: &[u8],
    resolved_fees: PumpFeesBps,
    base_mint_safety: MintQuoteSafety,
    quote_mint_safety: MintQuoteSafety,
    version: StateVersion,
) -> Result<PumpState, SnapshotError> {
    require_quote_safe(base_mint_safety)?;
    require_quote_safe(quote_mint_safety)?;

    let pool = decode_pump_pool(pool_data)?;
    let base_vault = decode_token_account_base(base_vault_data)?;
    let quote_vault = decode_token_account_base(quote_vault_data)?;

    require_initialized(base_vault.state)?;
    require_initialized(quote_vault.state)?;

    if base_vault.mint != pool.base_mint || quote_vault.mint != pool.quote_mint {
        return Err(SnapshotError::VaultMintMismatch);
    }

    Ok(build_pump_quote_state(
        &pool,
        base_vault.amount,
        quote_vault.amount,
        resolved_fees,
        version,
    ))
}

pub fn assemble_pump_state_with_mints(
    pool_data: &[u8],
    base_vault_data: &[u8],
    quote_vault_data: &[u8],
    base_mint_data: &[u8],
    quote_mint_data: &[u8],
    resolved_fees: PumpFeesBps,
    version: StateVersion,
) -> Result<(PumpState, MintQuoteInfo), SnapshotError> {
    let (base_mint, quote_mint) = inspect_pair_mints(base_mint_data, quote_mint_data)?;

    let state = assemble_pump_state(
        pool_data,
        base_vault_data,
        quote_vault_data,
        resolved_fees,
        base_mint.safety,
        quote_mint.safety,
        version,
    )?;

    Ok((state, base_mint))
}

/// Assemble a Pump quote snapshot directly from raw pool/vault/mint/FeeConfig
/// account data.
///
/// Canonical-pool and quote-mint classification are discovery concerns, so the
/// already-classified fee schedule is supplied by the caller.
pub fn assemble_pump_state_from_config(
    pool_data: &[u8],
    fee_config_data: &[u8],
    base_vault_data: &[u8],
    quote_vault_data: &[u8],
    base_mint_data: &[u8],
    quote_mint_data: &[u8],
    fee_schedule: PumpFeeSchedule,
    creator_fee_configurable: bool,
    version: StateVersion,
) -> Result<(PumpState, PumpSnapshotMeta), SnapshotError> {
    let pool = decode_pump_pool(pool_data)?;
    let fee_config = decode_fee_config(fee_config_data)?;
    let base_vault = decode_token_account_base(base_vault_data)?;
    let quote_vault = decode_token_account_base(quote_vault_data)?;
    let (base_mint, quote_mint) = inspect_pair_mints(base_mint_data, quote_mint_data)?;

    require_quote_safe(base_mint.safety)?;
    require_quote_safe(quote_mint.safety)?;
    require_initialized(base_vault.state)?;
    require_initialized(quote_vault.state)?;

    if base_vault.mint != pool.base_mint || quote_vault.mint != pool.quote_mint {
        return Err(SnapshotError::VaultMintMismatch);
    }

    let effective_quote =
        effective_quote_reserve(quote_vault.amount, pool.virtual_quote_reserves)?;

    let market_cap = pool_market_cap(
        base_mint.supply,
        base_vault.amount,
        effective_quote,
        pool.is_mayhem_mode,
    )?;

    let has_coin_creator = pool.coin_creator != [0u8; 32];

    let resolved_fees = resolve_pool_fees(
        &fee_config.config,
        fee_schedule,
        market_cap,
        has_coin_creator,
        creator_fee_configurable,
        pool.creator_fee_bps,
        pool.is_cashback_coin,
    )?;

    let state = build_pump_quote_state(
        &pool,
        base_vault.amount,
        quote_vault.amount,
        resolved_fees,
        version,
    );

    Ok((
        state,
        PumpSnapshotMeta {
            base_mint,
            quote_mint,
            market_cap,
            resolved_fees,
        },
    ))
}

pub fn assemble_raydium_state(
    pool_data: &[u8],
    amm_config_data: &[u8],
    vault_0_data: &[u8],
    vault_1_data: &[u8],
    mint_0_safety: MintQuoteSafety,
    mint_1_safety: MintQuoteSafety,
    version: StateVersion,
) -> Result<RaydiumCpmmState, SnapshotError> {
    require_quote_safe(mint_0_safety)?;
    require_quote_safe(mint_1_safety)?;

    let pool = decode_pool_state(pool_data)?;
    let config = decode_amm_config(amm_config_data)?;
    let vault_0 = decode_token_account_base(vault_0_data)?;
    let vault_1 = decode_token_account_base(vault_1_data)?;

    require_initialized(vault_0.state)?;
    require_initialized(vault_1.state)?;

    if vault_0.mint != pool.token_0_mint || vault_1.mint != pool.token_1_mint {
        return Err(SnapshotError::VaultMintMismatch);
    }

    Ok(build_raydium_quote_state(
        &pool,
        config,
        vault_0.amount,
        vault_1.amount,
        version,
    )?)
}

pub fn assemble_raydium_state_with_mints(
    pool_data: &[u8],
    amm_config_data: &[u8],
    vault_0_data: &[u8],
    vault_1_data: &[u8],
    mint_0_data: &[u8],
    mint_1_data: &[u8],
    version: StateVersion,
) -> Result<RaydiumCpmmState, SnapshotError> {
    let (mint_0, mint_1) = inspect_pair_mints(mint_0_data, mint_1_data)?;

    assemble_raydium_state(
        pool_data,
        amm_config_data,
        vault_0_data,
        vault_1_data,
        mint_0.safety,
        mint_1.safety,
        version,
    )
}
