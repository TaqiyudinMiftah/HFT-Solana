pub mod cpmm;

use thiserror::Error;

use crate::types::StateVersion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub amount_in: u64,
    pub amount_out: u64,
    pub dex_fee: u64,
    pub cashback: u64,
    pub version: StateVersion,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QuoteError {
    #[error("input amount is zero")]
    ZeroInput,
    #[error("pool has insufficient liquidity")]
    InsufficientLiquidity,
    #[error("integer math overflow")]
    MathOverflow,
}
