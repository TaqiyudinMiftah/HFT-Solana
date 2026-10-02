use crate::{
    decode::meteora_damm::{MeteoraDynamicFee, MeteoraPoolFees},
    quote::QuoteError,
    types::Direction,
};

pub const FEE_DENOMINATOR: u64 = 1_000_000_000;
pub const MAX_FEE_NUMERATOR_V0: u64 = 500_000_000;
pub const MAX_FEE_NUMERATOR_V1: u64 = 990_000_000;
pub const MAX_BASIS_POINT: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeteoraFeeMode {
    pub fees_on_input: bool,
    pub fees_on_token_a: bool,
    pub has_referral: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeteoraSplitFees {
    pub claiming_fee: u64,
    pub compounding_fee: u64,
    pub protocol_fee: u64,
    pub referral_fee: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeteoraFeeOnAmount {
    pub amount: u64,
    pub trading_fee: u64,
    pub split: MeteoraSplitFees,
}

#[inline(always)]
fn mul_div_floor(a: u128, b: u128, d: u128) -> Result<u128, QuoteError> {
    a.checked_mul(b)
        .ok_or(QuoteError::MathOverflow)?
        .checked_div(d)
        .ok_or(QuoteError::MathOverflow)
}

#[inline(always)]
fn mul_div_ceil(a: u128, b: u128, d: u128) -> Result<u128, QuoteError> {
    if d == 0 {
        return Err(QuoteError::InvalidFee);
    }

    let product = a.checked_mul(b).ok_or(QuoteError::MathOverflow)?;
    let rounded = product.checked_add(d - 1).ok_or(QuoteError::MathOverflow)?;
    Ok(rounded / d)
}

pub fn max_fee_numerator(fee_version: u8) -> Result<u64, QuoteError> {
    match fee_version {
        0 => Ok(MAX_FEE_NUMERATOR_V0),
        1 => Ok(MAX_FEE_NUMERATOR_V1),
        _ => Err(QuoteError::InvalidFee),
    }
}

/// Mirrors DynamicFeeStruct::get_variable_fee from the current cp-amm program.
pub fn variable_fee_numerator(dynamic: &MeteoraDynamicFee) -> Result<u128, QuoteError> {
    if !dynamic.initialized {
        return Ok(0);
    }

    let vfa_bin = dynamic
        .volatility_accumulator
        .checked_mul(dynamic.bin_step as u128)
        .ok_or(QuoteError::MathOverflow)?;
    let square_vfa_bin = vfa_bin
        .checked_mul(vfa_bin)
        .ok_or(QuoteError::MathOverflow)?;
    let variable = square_vfa_bin
        .checked_mul(dynamic.variable_fee_control as u128)
        .ok_or(QuoteError::MathOverflow)?;

    variable
        .checked_add(99_999_999_999)
        .ok_or(QuoteError::MathOverflow)?
        .checked_div(100_000_000_000)
        .ok_or(QuoteError::MathOverflow)
}

pub fn total_fee_numerator(
    base_fee_numerator: u64,
    dynamic: &MeteoraDynamicFee,
    fee_version: u8,
) -> Result<u64, QuoteError> {
    let dynamic = variable_fee_numerator(dynamic)?;
    let total = (base_fee_numerator as u128)
        .checked_add(dynamic)
        .ok_or(QuoteError::MathOverflow)?;
    let cap = max_fee_numerator(fee_version)? as u128;

    u64::try_from(total.min(cap)).map_err(|_| QuoteError::MathOverflow)
}

/// Convert an amount that includes trading fee into the amount available to
/// the liquidity curve. The program rounds the fee upward.
pub fn excluded_fee_amount(
    trade_fee_numerator: u64,
    included_fee_amount: u64,
) -> Result<(u64, u64), QuoteError> {
    if trade_fee_numerator >= FEE_DENOMINATOR {
        return Err(QuoteError::InvalidFee);
    }

    let trading_fee = mul_div_ceil(
        included_fee_amount as u128,
        trade_fee_numerator as u128,
        FEE_DENOMINATOR as u128,
    )?;
    let trading_fee = u64::try_from(trading_fee).map_err(|_| QuoteError::MathOverflow)?;
    let excluded = included_fee_amount
        .checked_sub(trading_fee)
        .ok_or(QuoteError::InvalidFee)?;

    Ok((excluded, trading_fee))
}

/// Convert a curve amount that excludes trading fee into the user-facing
/// included amount. The program rounds the included amount upward.
pub fn included_fee_amount(
    trade_fee_numerator: u64,
    excluded_fee_amount: u64,
) -> Result<(u64, u64), QuoteError> {
    if trade_fee_numerator >= FEE_DENOMINATOR {
        return Err(QuoteError::InvalidFee);
    }

    let denominator = FEE_DENOMINATOR - trade_fee_numerator;
    let included = mul_div_ceil(
        excluded_fee_amount as u128,
        FEE_DENOMINATOR as u128,
        denominator as u128,
    )?;
    let included = u64::try_from(included).map_err(|_| QuoteError::MathOverflow)?;
    let fee = included
        .checked_sub(excluded_fee_amount)
        .ok_or(QuoteError::MathOverflow)?;

    Ok((included, fee))
}

pub fn split_fees(
    fees: &MeteoraPoolFees,
    fee_amount: u64,
    has_referral: bool,
) -> Result<MeteoraSplitFees, QuoteError> {
    let protocol_fee = mul_div_floor(fee_amount as u128, fees.protocol_fee_percent as u128, 100)?;
    let protocol_fee = u64::try_from(protocol_fee).map_err(|_| QuoteError::MathOverflow)?;
    let lp_trading_fee = fee_amount
        .checked_sub(protocol_fee)
        .ok_or(QuoteError::MathOverflow)?;

    let compounding_fee = if fees.compounding_fee_bps > 0 {
        mul_div_floor(
            lp_trading_fee as u128,
            fees.compounding_fee_bps as u128,
            MAX_BASIS_POINT as u128,
        )?
    } else {
        0
    };
    let compounding_fee = u64::try_from(compounding_fee).map_err(|_| QuoteError::MathOverflow)?;
    let claiming_fee = lp_trading_fee
        .checked_sub(compounding_fee)
        .ok_or(QuoteError::MathOverflow)?;

    let referral_fee = if has_referral {
        mul_div_floor(protocol_fee as u128, fees.referral_fee_percent as u128, 100)?
    } else {
        0
    };
    let referral_fee = u64::try_from(referral_fee).map_err(|_| QuoteError::MathOverflow)?;
    let protocol_fee = protocol_fee
        .checked_sub(referral_fee)
        .ok_or(QuoteError::MathOverflow)?;

    Ok(MeteoraSplitFees {
        claiming_fee,
        compounding_fee,
        protocol_fee,
        referral_fee,
    })
}

pub fn fee_on_amount(
    fees: &MeteoraPoolFees,
    amount: u64,
    trade_fee_numerator: u64,
    has_referral: bool,
) -> Result<MeteoraFeeOnAmount, QuoteError> {
    let (amount, trading_fee) = excluded_fee_amount(trade_fee_numerator, amount)?;
    let split = split_fees(fees, trading_fee, has_referral)?;

    Ok(MeteoraFeeOnAmount {
        amount,
        trading_fee,
        split,
    })
}

/// Mirrors FeeMode::get_fee_mode from the current cp-amm program.
///
/// collect_fee_mode:
/// 0 = BothToken
/// 1 = OnlyB
/// 2 = Compounding
pub fn fee_mode(
    collect_fee_mode: u8,
    direction: Direction,
    has_referral: bool,
) -> Result<MeteoraFeeMode, QuoteError> {
    let (fees_on_input, fees_on_token_a) = match (collect_fee_mode, direction) {
        (0, Direction::AtoB) => (false, false),
        (0, Direction::BtoA) => (false, true),
        (1 | 2, Direction::AtoB) => (false, false),
        (1 | 2, Direction::BtoA) => (true, false),
        _ => return Err(QuoteError::InvalidFee),
    };

    Ok(MeteoraFeeMode {
        fees_on_input,
        fees_on_token_a,
        has_referral,
    })
}
