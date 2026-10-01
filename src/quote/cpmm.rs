use crate::{
    quote::{Quote, QuoteError},
    types::StateVersion,
};

const PPM_DENOMINATOR: u128 = 1_000_000;

#[inline(always)]
fn mul_div_floor(a: u128, b: u128, d: u128) -> Result<u128, QuoteError> {
    a.checked_mul(b)
        .ok_or(QuoteError::MathOverflow)?
        .checked_div(d)
        .ok_or(QuoteError::MathOverflow)
}

#[inline(always)]
pub fn fee_floor(amount: u64, fee_ppm: u64) -> Result<u64, QuoteError> {
    let fee = mul_div_floor(amount as u128, fee_ppm as u128, PPM_DENOMINATOR)?;
    u64::try_from(fee).map_err(|_| QuoteError::MathOverflow)
}

/// Constant-product exact-input quote.
///
/// This is a generic research primitive, not a claim of byte-for-byte
/// parity with any specific DEX. DEX adapters must wrap this and prove
/// integer/rounding parity against program or official SDK fixtures.
#[inline(always)]
pub fn quote_xyk_exact_in(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    fee_ppm: u64,
    version: StateVersion,
) -> Result<Quote, QuoteError> {
    if amount_in == 0 {
        return Err(QuoteError::ZeroInput);
    }
    if reserve_in == 0 || reserve_out == 0 {
        return Err(QuoteError::InsufficientLiquidity);
    }

    let fee = fee_floor(amount_in, fee_ppm)?;
    let effective_in = amount_in
        .checked_sub(fee)
        .ok_or(QuoteError::MathOverflow)?;

    let numerator = (effective_in as u128)
        .checked_mul(reserve_out as u128)
        .ok_or(QuoteError::MathOverflow)?;
    let denominator = (reserve_in as u128)
        .checked_add(effective_in as u128)
        .ok_or(QuoteError::MathOverflow)?;

    let amount_out =
        u64::try_from(numerator / denominator).map_err(|_| QuoteError::MathOverflow)?;

    Ok(Quote {
        amount_in,
        amount_out,
        dex_fee: fee,
        cashback: 0,
        version,
    })
}
