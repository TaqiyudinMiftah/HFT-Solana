use crate::{
    quote::{CashbackLocation, Quote, QuoteError},
    types::StateVersion,
};

const DENOMINATOR: u128 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaydiumFees {
    pub trade_fee_rate: u64,
    pub creator_fee_rate: u64,
    pub protocol_fee_rate: u64,
    pub fund_fee_rate: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaydiumQuote {
    pub quote: Quote,
    pub trade_fee: u64,
    pub creator_fee: u64,
    pub protocol_fee: u64,
    pub fund_fee: u64,
    pub curve_output: u64,
}

#[inline(always)]
fn ceil_rate(amount: u128, rate: u64) -> Result<u128, QuoteError> {
    if rate == 0 {
        return Ok(0);
    }
    amount
        .checked_mul(rate as u128)
        .ok_or(QuoteError::MathOverflow)?
        .checked_add(DENOMINATOR - 1)
        .ok_or(QuoteError::MathOverflow)?
        .checked_div(DENOMINATOR)
        .ok_or(QuoteError::MathOverflow)
}

#[inline(always)]
fn floor_rate(amount: u128, rate: u64) -> Result<u128, QuoteError> {
    amount
        .checked_mul(rate as u128)
        .ok_or(QuoteError::MathOverflow)?
        .checked_div(DENOMINATOR)
        .ok_or(QuoteError::MathOverflow)
}

pub fn quote_base_input(
    amount_in: u64,
    input_vault_amount: u64,
    output_vault_amount: u64,
    fees: RaydiumFees,
    creator_fee_on_input: bool,
    version: StateVersion,
) -> Result<RaydiumQuote, QuoteError> {
    if amount_in == 0 {
        return Err(QuoteError::ZeroInput);
    }
    if input_vault_amount == 0 || output_vault_amount == 0 {
        return Err(QuoteError::InsufficientLiquidity);
    }

    let input = amount_in as u128;
    let mut creator_fee = 0u128;

    let (trade_fee, curve_input) = if creator_fee_on_input {
        let combined_rate = fees
            .trade_fee_rate
            .checked_add(fees.creator_fee_rate)
            .ok_or(QuoteError::InvalidFee)?;

        let total_fee = ceil_rate(input, combined_rate)?;
        if total_fee > input {
            return Err(QuoteError::InvalidFee);
        }

        if combined_rate != 0 && fees.creator_fee_rate != 0 {
            creator_fee = total_fee
                .checked_mul(fees.creator_fee_rate as u128)
                .ok_or(QuoteError::MathOverflow)?
                / combined_rate as u128;
        }

        (total_fee - creator_fee, input - total_fee)
    } else {
        let trade_fee = ceil_rate(input, fees.trade_fee_rate)?;
        if trade_fee > input {
            return Err(QuoteError::InvalidFee);
        }
        (trade_fee, input - trade_fee)
    };

    let curve_output = curve_input
        .checked_mul(output_vault_amount as u128)
        .ok_or(QuoteError::MathOverflow)?
        / ((input_vault_amount as u128)
            .checked_add(curve_input)
            .ok_or(QuoteError::MathOverflow)?);

    let output = if creator_fee_on_input {
        curve_output
    } else {
        creator_fee = ceil_rate(curve_output, fees.creator_fee_rate)?;
        curve_output
            .checked_sub(creator_fee)
            .ok_or(QuoteError::InvalidFee)?
    };

    let protocol_fee = floor_rate(trade_fee, fees.protocol_fee_rate)?;
    let fund_fee = floor_rate(trade_fee, fees.fund_fee_rate)?;

    let trade_fee_u64 = u64::try_from(trade_fee).map_err(|_| QuoteError::MathOverflow)?;
    let creator_fee_u64 = u64::try_from(creator_fee).map_err(|_| QuoteError::MathOverflow)?;

    Ok(RaydiumQuote {
        quote: Quote {
            amount_in,
            amount_out: u64::try_from(output).map_err(|_| QuoteError::MathOverflow)?,
            dex_fee: trade_fee_u64
                .checked_add(creator_fee_u64)
                .ok_or(QuoteError::MathOverflow)?,
            cashback: 0,
            cashback_location: CashbackLocation::None,
            version,
        },
        trade_fee: trade_fee_u64,
        creator_fee: creator_fee_u64,
        protocol_fee: u64::try_from(protocol_fee).map_err(|_| QuoteError::MathOverflow)?,
        fund_fee: u64::try_from(fund_fee).map_err(|_| QuoteError::MathOverflow)?,
        curve_output: u64::try_from(curve_output).map_err(|_| QuoteError::MathOverflow)?,
    })
}
