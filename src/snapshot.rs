use thiserror::Error;

use crate::{
    decode::{
        pump::{build_quote_state as build_pump_quote_state, decode_pool as decode_pump_pool},
        raydium::{
            build_quote_state as build_raydium_quote_state, decode_amm_config,
            decode_pool_state,
        },
        token::{decode_token_account_base, TokenAccountState},
        DecodeError,
    },
    quote::pump::PumpFeesBps,
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SnapshotError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("vault mint does not match pool mint")]
    VaultMintMismatch,
    #[error("vault token account is not initialized")]
    VaultNotInitialized,
    #[error("mint has unsupported or unresolved quote-affecting extensions")]
    UnsupportedMintBehavior,
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
