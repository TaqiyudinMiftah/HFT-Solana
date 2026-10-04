use thiserror::Error;

#[cfg(feature = "meteora-dlmm")]
use solana_sdk_v2::{account::Account, pubkey::Pubkey};
#[cfg(feature = "meteora-dlmm")]
use std::collections::HashMap;

use crate::{
    decode::{
        mint::{inspect_mint, MintInspectError, MintQuoteInfo},
        pump::{build_quote_state as build_pump_quote_state, decode_pool as decode_pump_pool},
        pump_fee::decode_fee_config,
        pump_global::{decode_pump_amm_global_config, decode_pump_global},
        raydium::{
            build_quote_state as build_raydium_quote_state, decode_amm_config, decode_pool_state,
        },
        token::{decode_token_account_base, TokenAccountState},
        DecodeError,
    },
    quote::{
        pump::{
            effective_quote_reserve, pool_market_cap, resolve_pool_fees, PumpFeeSchedule,
            PumpFeesBps,
        },
        pump_identity::classify_fee_schedule,
        QuoteError,
    },
    state::{PumpState, RaydiumCpmmState},
    types::{Direction, StateVersion},
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PumpAutoSnapshotMeta {
    pub base_mint: MintQuoteInfo,
    pub quote_mint: MintQuoteInfo,
    pub market_cap: u128,
    pub fee_schedule: PumpFeeSchedule,
    pub resolved_fees: PumpFeesBps,
    pub pump_creator_fee_configurable: bool,
    pub pump_max_configurable_creator_fee_bps: u64,
    pub amm_creator_fee_configurable: bool,
    pub amm_max_configurable_creator_fee_bps: u64,
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
    #[cfg(feature = "meteora-dlmm")]
    #[error("DLMM pair mint does not match configured mint")]
    DlmmMintMismatch,
    #[cfg(feature = "meteora-dlmm")]
    #[error("DLMM quote snapshot needs at least one bin array")]
    DlmmMissingBinArray,
    #[cfg(feature = "meteora-dlmm")]
    #[error("DLMM bin array belongs to a different pair")]
    DlmmBinArrayPairMismatch,
    #[cfg(feature = "meteora-dlmm")]
    #[error("DLMM bitmap extension belongs to a different pair")]
    DlmmBitmapPairMismatch,
    #[cfg(feature = "meteora-dlmm")]
    #[error("duplicate DLMM bin-array account")]
    DlmmDuplicateBinArray,
    #[cfg(feature = "meteora-dlmm")]
    #[error(
        "DLMM configured bin window is incomplete for {direction:?}: {missing} required arrays missing"
    )]
    DlmmIncompleteBinWindow {
        direction: Direction,
        missing: usize,
    },
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

    let effective_quote = effective_quote_reserve(quote_vault.amount, pool.virtual_quote_reserves)?;

    let market_cap = pool_market_cap(
        base_mint.supply,
        base_vault.amount,
        effective_quote,
        pool.is_mayhem_mode,
    )?;

    let has_coin_creator = pool.coin_creator != [0u8; 32];

    let _ = creator_fee_configurable;
    let resolved_fees = resolve_pool_fees(
        &fee_config.config,
        fee_schedule,
        market_cap,
        has_coin_creator,
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

/// Assemble a PumpSwap quote state without caller-supplied fee classification.
///
/// Canonical-pool identity is derived from the Pump program PDA. Canonical
/// quote assets are then classified as WSOL, Pump-Global-whitelisted stable,
/// or exotic. Creator-fee global flags are returned as diagnostics only;
/// quoting follows the creator fee already persisted in the pool.
pub fn assemble_pump_state_auto(
    pool_data: &[u8],
    fee_config_data: &[u8],
    pump_global_data: &[u8],
    pump_amm_global_config_data: &[u8],
    base_vault_data: &[u8],
    quote_vault_data: &[u8],
    base_mint_data: &[u8],
    quote_mint_data: &[u8],
    version: StateVersion,
) -> Result<(PumpState, PumpAutoSnapshotMeta), SnapshotError> {
    let pool = decode_pump_pool(pool_data)?;
    let fee_config = decode_fee_config(fee_config_data)?;
    let pump_global = decode_pump_global(pump_global_data)?;
    let amm_global = decode_pump_amm_global_config(pump_amm_global_config_data)?;
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

    let effective_quote = effective_quote_reserve(quote_vault.amount, pool.virtual_quote_reserves)?;
    let market_cap = pool_market_cap(
        base_mint.supply,
        base_vault.amount,
        effective_quote,
        pool.is_mayhem_mode,
    )?;

    let fee_schedule = classify_fee_schedule(&pool, &pump_global);
    let has_coin_creator = pool.coin_creator != [0u8; 32];

    let resolved_fees = resolve_pool_fees(
        &fee_config.config,
        fee_schedule,
        market_cap,
        has_coin_creator,
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
        PumpAutoSnapshotMeta {
            base_mint,
            quote_mint,
            market_cap,
            fee_schedule,
            resolved_fees,
            pump_creator_fee_configurable: pump_global.creator_fee_configurable,
            pump_max_configurable_creator_fee_bps: pump_global.max_configurable_creator_fee_bps,
            amm_creator_fee_configurable: amm_global.creator_fee_configurable,
            amm_max_configurable_creator_fee_bps: amm_global.max_configurable_creator_fee_bps,
        },
    ))
}

#[cfg(feature = "meteora-dlmm")]
pub fn assemble_meteora_dlmm_quote_state(
    lb_pair_key: [u8; 32],
    lb_pair_data: &[u8],
    bin_arrays: &[([u8; 32], &[u8])],
    bitmap_extension_data: Option<&[u8]>,
    mint_x_key: [u8; 32],
    mint_x_owner: [u8; 32],
    mint_x_data: &[u8],
    mint_y_key: [u8; 32],
    mint_y_owner: [u8; 32],
    mint_y_data: &[u8],
) -> Result<crate::quote::meteora_dlmm::MeteoraDlmmQuoteState, SnapshotError> {
    if bin_arrays.is_empty() {
        return Err(SnapshotError::DlmmMissingBinArray);
    }

    let pair = crate::decode::meteora_dlmm::decode_lb_pair(lb_pair_data)?;
    let (mint_x_info, mint_y_info) = inspect_pair_mints(mint_x_data, mint_y_data)?;
    require_quote_safe(mint_x_info.safety)?;
    require_quote_safe(mint_y_info.safety)?;

    if pair.token_x_mint.to_bytes() != mint_x_key || pair.token_y_mint.to_bytes() != mint_y_key {
        return Err(SnapshotError::DlmmMintMismatch);
    }

    let pair_pubkey = Pubkey::new_from_array(lb_pair_key);
    let mut decoded_bin_arrays = HashMap::with_capacity(bin_arrays.len());

    for (bin_array_key, data) in bin_arrays {
        let bin_array = crate::decode::meteora_dlmm::decode_bin_array(data)?;
        if bin_array.lb_pair.to_bytes() != lb_pair_key {
            return Err(SnapshotError::DlmmBinArrayPairMismatch);
        }

        if decoded_bin_arrays
            .insert(Pubkey::new_from_array(*bin_array_key), bin_array)
            .is_some()
        {
            return Err(SnapshotError::DlmmDuplicateBinArray);
        }
    }

    let bitmap_extension = match bitmap_extension_data {
        Some(data) => {
            let bitmap = crate::decode::meteora_dlmm::decode_bitmap_extension(data)?;
            if bitmap.lb_pair.to_bytes() != lb_pair_key {
                return Err(SnapshotError::DlmmBitmapPairMismatch);
            }
            Some(bitmap)
        }
        None => None,
    };

    let mint_x_account = Account {
        lamports: 0,
        data: mint_x_data.to_vec(),
        owner: Pubkey::new_from_array(mint_x_owner),
        executable: false,
        rent_epoch: 0,
    };
    let mint_y_account = Account {
        lamports: 0,
        data: mint_y_data.to_vec(),
        owner: Pubkey::new_from_array(mint_y_owner),
        executable: false,
        rent_epoch: 0,
    };

    Ok(crate::quote::meteora_dlmm::MeteoraDlmmQuoteState {
        lb_pair_pubkey: pair_pubkey,
        lb_pair: pair,
        bin_arrays: decoded_bin_arrays,
        bitmap_extension,
        mint_x_account,
        mint_y_account,
    })
}


#[cfg(feature = "meteora-dlmm")]
pub fn validate_meteora_dlmm_bin_window(
    state: &crate::quote::meteora_dlmm::MeteoraDlmmQuoteState,
    take_count: u8,
) -> Result<(), SnapshotError> {
    for direction in [Direction::AtoB, Direction::BtoA] {
        let missing =
            crate::quote::meteora_dlmm::missing_bin_array_pubkeys(state, direction, take_count)?;

        if !missing.is_empty() {
            return Err(SnapshotError::DlmmIncompleteBinWindow {
                direction,
                missing: missing.len(),
            });
        }
    }

    Ok(())
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

#[cfg(feature = "meteora-damm")]
pub fn assemble_meteora_damm_state_with_mints(
    pool_data: &[u8],
    vault_a_data: &[u8],
    vault_b_data: &[u8],
    mint_a_data: &[u8],
    mint_b_data: &[u8],
    version: StateVersion,
) -> Result<crate::state::MeteoraDammState, SnapshotError> {
    let decoded = crate::decode::meteora_damm::decode_pool(pool_data)?;
    let official = crate::decode::meteora_damm::decode_official_pool(pool_data)?;
    let vault_a = decode_token_account_base(vault_a_data)?;
    let vault_b = decode_token_account_base(vault_b_data)?;
    let (mint_a, mint_b) = inspect_pair_mints(mint_a_data, mint_b_data)?;

    require_quote_safe(mint_a.safety)?;
    require_quote_safe(mint_b.safety)?;
    require_initialized(vault_a.state)?;
    require_initialized(vault_b.state)?;

    if vault_a.mint != decoded.token_a_mint || vault_b.mint != decoded.token_b_mint {
        return Err(SnapshotError::VaultMintMismatch);
    }

    Ok(crate::state::MeteoraDammState {
        version,
        pool: std::sync::Arc::new(official),
    })
}
