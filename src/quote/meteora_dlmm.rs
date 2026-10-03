use std::collections::HashMap;

use meteora_dlmm_commons::{
    dlmm::accounts::{BinArray, BinArrayBitmapExtension, LbPair},
    quote::quote_exact_in,
};
use solana_sdk_v2::{account::Account, clock::Clock, pubkey::Pubkey};

use crate::{
    quote::{CashbackLocation, Quote, QuoteError},
    types::{Direction, StateVersion},
};

#[derive(Clone)]
pub struct MeteoraDlmmQuoteState {
    pub lb_pair_pubkey: Pubkey,
    pub lb_pair: LbPair,
    pub bin_arrays: HashMap<Pubkey, BinArray>,
    pub bitmap_extension: Option<BinArrayBitmapExtension>,
    pub mint_x_account: Account,
    pub mint_y_account: Account,
}

pub fn quote_exact_in_official(
    state: &MeteoraDlmmQuoteState,
    amount_in: u64,
    direction: Direction,
    current_timestamp: u64,
    current_slot: u64,
    current_epoch: u64,
    version: StateVersion,
) -> Result<Quote, QuoteError> {
    if amount_in == 0 {
        return Err(QuoteError::ZeroInput);
    }

    let unix_timestamp = i64::try_from(current_timestamp).map_err(|_| QuoteError::MathOverflow)?;
    let clock = Clock {
        slot: current_slot,
        epoch: current_epoch,
        unix_timestamp,
        ..Clock::default()
    };

    let result = quote_exact_in(
        state.lb_pair_pubkey,
        &state.lb_pair,
        amount_in,
        matches!(direction, Direction::AtoB),
        state.bin_arrays.clone(),
        state.bitmap_extension.as_ref(),
        &clock,
        &state.mint_x_account,
        &state.mint_y_account,
    )
    .map_err(|_| QuoteError::MeteoraDlmmQuote)?;

    Ok(Quote {
        amount_in,
        amount_out: result.amount_out,
        dex_fee: result.fee,
        cashback: 0,
        cashback_location: CashbackLocation::None,
        version,
    })
}
