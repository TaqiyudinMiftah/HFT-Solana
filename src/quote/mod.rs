pub mod cpmm;
pub mod pump;
pub mod raydium;

use thiserror::Error;

use crate::types::StateVersion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CashbackLocation {
    None,
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub amount_in: u64,
    pub amount_out: u64,
    pub dex_fee: u64,
    pub cashback: u64,
    pub cashback_location: CashbackLocation,
    pub version: StateVersion,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QuoteError {
    #[error("input amount is zero")]
    ZeroInput,
    #[error("pool has insufficient liquidity")]
    InsufficientLiquidity,
    #[error("invalid fee configuration")]
    InvalidFee,
    #[error("effective reserve is not positive")]
    InvalidEffectiveReserve,
    #[error("integer math overflow")]
    MathOverflow,
}
