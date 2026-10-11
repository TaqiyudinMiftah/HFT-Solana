use crate::{
    quote::{CashbackLocation, Quote, QuoteError},
    types::StateVersion,
};

const BPS_DENOMINATOR: u128 = 10_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PumpFeesBps {
    pub lp_fee_bps: u64,
    pub protocol_fee_bps: u64,
    pub creator_fee_bps: u64,
}

impl PumpFeesBps {
    pub fn is_zero(self) -> bool {
        self.lp_fee_bps == 0 && self.protocol_fee_bps == 0 && self.creator_fee_bps == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PumpFeeTier {
    pub market_cap_threshold: u128,
    pub fees: PumpFeesBps,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PumpFeeConfig {
    pub flat_fees: PumpFeesBps,
    pub fee_tiers: Vec<PumpFeeTier>,
    pub stable_fee_tiers: Vec<PumpFeeTier>,
    pub exotic_flat_fees: PumpFeesBps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumpFeeSchedule {
    NonCanonical,
    CanonicalSolLike,
    CanonicalStable,
    CanonicalExotic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PumpQuote {
    pub quote: Quote,
    pub gross_quote_amount: u64,
    pub effective_quote_amount: u64,
    pub lp_fee: u64,
    pub protocol_fee: u64,
    pub creator_fee: u64,
}

#[inline(always)]
fn ceil_fee(amount: u128, bps: u64) -> Result<u128, QuoteError> {
    if bps == 0 {
        return Ok(0);
    }
    amount
        .checked_mul(bps as u128)
        .ok_or(QuoteError::MathOverflow)?
        .checked_add(BPS_DENOMINATOR - 1)
        .ok_or(QuoteError::MathOverflow)?
        .checked_div(BPS_DENOMINATOR)
        .ok_or(QuoteError::MathOverflow)
}

pub fn effective_quote_reserve(raw_quote: u64, virtual_quote: i128) -> Result<u128, QuoteError> {
    let effective = (raw_quote as i128)
        .checked_add(virtual_quote)
        .ok_or(QuoteError::MathOverflow)?;

    if effective <= 0 {
        return Err(QuoteError::InvalidEffectiveReserve);
    }
    Ok(effective as u128)
}

pub fn pool_market_cap(
    base_mint_supply: u64,
    base_reserve: u64,
    effective_quote_reserve: u128,
    is_mayhem_mode: bool,
) -> Result<u128, QuoteError> {
    if base_reserve == 0 {
        return Err(QuoteError::InsufficientLiquidity);
    }

    let supply = if is_mayhem_mode {
        1_000_000_000_000_000u128
    } else {
        base_mint_supply as u128
    };

    effective_quote_reserve
        .checked_mul(supply)
        .ok_or(QuoteError::MathOverflow)?
        .checked_div(base_reserve as u128)
        .ok_or(QuoteError::MathOverflow)
}

pub fn calculate_fee_tier(
    tiers: &[PumpFeeTier],
    market_cap: u128,
) -> Result<PumpFeesBps, QuoteError> {
    let first = tiers.first().ok_or(QuoteError::InvalidFee)?;

    if market_cap < first.market_cap_threshold {
        return Ok(first.fees);
    }

    Ok(tiers
        .iter()
        .rev()
        .find(|tier| market_cap >= tier.market_cap_threshold)
        .map(|tier| tier.fees)
        .unwrap_or(first.fees))
}

pub fn fees_for_schedule(
    config: &PumpFeeConfig,
    schedule: PumpFeeSchedule,
    market_cap: u128,
) -> Result<PumpFeesBps, QuoteError> {
    match schedule {
        PumpFeeSchedule::NonCanonical => Ok(config.flat_fees),
        PumpFeeSchedule::CanonicalSolLike => calculate_fee_tier(&config.fee_tiers, market_cap),
        PumpFeeSchedule::CanonicalStable => {
            let tiers = if config.stable_fee_tiers.is_empty() {
                &config.fee_tiers
            } else {
                &config.stable_fee_tiers
            };
            calculate_fee_tier(tiers, market_cap)
        }
        PumpFeeSchedule::CanonicalExotic => {
            if config.exotic_flat_fees.is_zero() {
                Ok(config.flat_fees)
            } else {
                Ok(config.exotic_flat_fees)
            }
        }
    }
}

/// Apply the pool-level creator-fee rules after selecting the schedule.
///
/// A pool without a coin creator pays no creator fee. A configurable nonzero
/// pool override replaces only the creator rate; cashback pools do not use
/// that override.
pub fn apply_creator_fee_rules(
    mut fees: PumpFeesBps,
    has_coin_creator: bool,
    pool_creator_fee_bps: u64,
    cashback_coin: bool,
) -> PumpFeesBps {
    if !has_coin_creator {
        fees.creator_fee_bps = 0;
        return fees;
    }

    // A nonzero creator fee is persisted in the pool at creation/edit time.
    // Zero means "use the schedule rate". The current global configurable flag
    // is therefore not consulted during quoting; doing so could change
    // historical pool economics retroactively.
    if pool_creator_fee_bps != 0 && !cashback_coin {
        fees.creator_fee_bps = pool_creator_fee_bps;
    }

    fees
}

pub fn resolve_pool_fees(
    config: &PumpFeeConfig,
    schedule: PumpFeeSchedule,
    market_cap: u128,
    has_coin_creator: bool,
    pool_creator_fee_bps: u64,
    cashback_coin: bool,
) -> Result<PumpFeesBps, QuoteError> {
    let scheduled = fees_for_schedule(config, schedule, market_cap)?;
    Ok(apply_creator_fee_rules(
        scheduled,
        has_coin_creator,
        pool_creator_fee_bps,
        cashback_coin,
    ))
}

/// Exact-quote buy: quote asset in, base asset out.
///
/// Current PumpSwap pricing uses the effective quote reserve (raw + virtual).
/// Fees are computed on the effective curve input; the SDK/on-chain path uses
/// a one-unit integer adjustment before the constant-product calculation.
pub fn buy_exact_quote_in(
    total_quote_in: u64,
    base_reserve: u64,
    raw_quote_reserve: u64,
    virtual_quote_reserves: i128,
    fees: PumpFeesBps,
    cashback_coin: bool,
    version: StateVersion,
) -> Result<PumpQuote, QuoteError> {
    if total_quote_in == 0 {
        return Err(QuoteError::ZeroInput);
    }
    if base_reserve == 0 {
        return Err(QuoteError::InsufficientLiquidity);
    }

    let quote_reserve = effective_quote_reserve(raw_quote_reserve, virtual_quote_reserves)?;
    let total_fee_bps = fees
        .lp_fee_bps
        .checked_add(fees.protocol_fee_bps)
        .and_then(|x| x.checked_add(fees.creator_fee_bps))
        .ok_or(QuoteError::InvalidFee)?;

    let denominator = BPS_DENOMINATOR
        .checked_add(total_fee_bps as u128)
        .ok_or(QuoteError::MathOverflow)?;

    let effective_quote = (total_quote_in as u128)
        .checked_mul(BPS_DENOMINATOR)
        .ok_or(QuoteError::MathOverflow)?
        / denominator;

    if effective_quote == 0 {
        return Err(QuoteError::InsufficientLiquidity);
    }

    let lp_fee = ceil_fee(effective_quote, fees.lp_fee_bps)?;
    let protocol_fee = ceil_fee(effective_quote, fees.protocol_fee_bps)?;
    let creator_fee = ceil_fee(effective_quote, fees.creator_fee_bps)?;

    let curve_input = effective_quote
        .checked_sub(1)
        .ok_or(QuoteError::InsufficientLiquidity)?;

    let base_out = (base_reserve as u128)
        .checked_mul(curve_input)
        .ok_or(QuoteError::MathOverflow)?
        / quote_reserve
            .checked_add(curve_input)
            .ok_or(QuoteError::MathOverflow)?;

    let total_fee = lp_fee
        .checked_add(protocol_fee)
        .and_then(|x| x.checked_add(creator_fee))
        .ok_or(QuoteError::MathOverflow)?;

    Ok(PumpQuote {
        quote: Quote {
            amount_in: total_quote_in,
            amount_out: u64::try_from(base_out).map_err(|_| QuoteError::MathOverflow)?,
            dex_fee: u64::try_from(total_fee).map_err(|_| QuoteError::MathOverflow)?,
            cashback: if cashback_coin {
                u64::try_from(creator_fee).map_err(|_| QuoteError::MathOverflow)?
            } else {
                0
            },
            cashback_location: if cashback_coin {
                CashbackLocation::Input
            } else {
                CashbackLocation::None
            },
            version,
        },
        gross_quote_amount: total_quote_in,
        effective_quote_amount: u64::try_from(effective_quote)
            .map_err(|_| QuoteError::MathOverflow)?,
        lp_fee: u64::try_from(lp_fee).map_err(|_| QuoteError::MathOverflow)?,
        protocol_fee: u64::try_from(protocol_fee).map_err(|_| QuoteError::MathOverflow)?,
        creator_fee: u64::try_from(creator_fee).map_err(|_| QuoteError::MathOverflow)?,
    })
}

/// Exact-base sell: base asset in, quote asset out.
pub fn sell_exact_base_in(
    base_in: u64,
    base_reserve: u64,
    raw_quote_reserve: u64,
    virtual_quote_reserves: i128,
    fees: PumpFeesBps,
    cashback_coin: bool,
    version: StateVersion,
) -> Result<PumpQuote, QuoteError> {
    if base_in == 0 {
        return Err(QuoteError::ZeroInput);
    }
    if base_reserve == 0 {
        return Err(QuoteError::InsufficientLiquidity);
    }

    let quote_reserve = effective_quote_reserve(raw_quote_reserve, virtual_quote_reserves)?;

    let gross_quote = (base_in as u128)
        .checked_mul(quote_reserve)
        .ok_or(QuoteError::MathOverflow)?
        / (base_reserve as u128)
            .checked_add(base_in as u128)
            .ok_or(QuoteError::MathOverflow)?;

    let lp_fee = ceil_fee(gross_quote, fees.lp_fee_bps)?;
    let protocol_fee = ceil_fee(gross_quote, fees.protocol_fee_bps)?;
    let creator_fee = ceil_fee(gross_quote, fees.creator_fee_bps)?;

    let total_fee = lp_fee
        .checked_add(protocol_fee)
        .and_then(|x| x.checked_add(creator_fee))
        .ok_or(QuoteError::MathOverflow)?;

    let net_quote = gross_quote
        .checked_sub(total_fee)
        .ok_or(QuoteError::InvalidFee)?;

    Ok(PumpQuote {
        quote: Quote {
            amount_in: base_in,
            amount_out: u64::try_from(net_quote).map_err(|_| QuoteError::MathOverflow)?,
            dex_fee: u64::try_from(total_fee).map_err(|_| QuoteError::MathOverflow)?,
            cashback: if cashback_coin {
                u64::try_from(creator_fee).map_err(|_| QuoteError::MathOverflow)?
            } else {
                0
            },
            cashback_location: if cashback_coin {
                CashbackLocation::Output
            } else {
                CashbackLocation::None
            },
            version,
        },
        gross_quote_amount: u64::try_from(gross_quote).map_err(|_| QuoteError::MathOverflow)?,
        effective_quote_amount: u64::try_from(gross_quote).map_err(|_| QuoteError::MathOverflow)?,
        lp_fee: u64::try_from(lp_fee).map_err(|_| QuoteError::MathOverflow)?,
        protocol_fee: u64::try_from(protocol_fee).map_err(|_| QuoteError::MathOverflow)?,
        creator_fee: u64::try_from(creator_fee).map_err(|_| QuoteError::MathOverflow)?,
    })
}
