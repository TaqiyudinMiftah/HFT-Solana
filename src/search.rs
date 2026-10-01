use crate::{
    graph::{Cycle, Edge},
    quote::{CashbackLocation, Quote, QuoteError},
    state::{PoolCell, PoolState},
    types::Direction,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteResult {
    pub amount_in: u64,
    pub amount_out: u64,
    pub total_fee: u64,
    pub base_cashback: u64,
    pub unconverted_cashback_legs: u8,
    pub gross_profit: i128,
    pub effective_profit: i128,
}

#[inline]
fn quote_edge(
    state: &PoolState,
    direction: Direction,
    amount_in: u64,
) -> Result<Quote, QuoteError> {
    match state {
        PoolState::Pump(s) => match direction {
            Direction::BtoA => crate::quote::pump::buy_exact_quote_in(
                amount_in,
                s.base_reserve,
                s.raw_quote_reserve,
                s.virtual_quote_reserves,
                s.fees,
                s.cashback_coin,
                s.version,
            )
            .map(|q| q.quote),
            Direction::AtoB => crate::quote::pump::sell_exact_base_in(
                amount_in,
                s.base_reserve,
                s.raw_quote_reserve,
                s.virtual_quote_reserves,
                s.fees,
                s.cashback_coin,
                s.version,
            )
            .map(|q| q.quote),
        },

        PoolState::RaydiumCpmm(s) => {
            let (reserve_in, reserve_out) = match direction {
                Direction::AtoB => (s.reserve_a, s.reserve_b),
                Direction::BtoA => (s.reserve_b, s.reserve_a),
            };

            crate::quote::raydium::quote_base_input(
                amount_in,
                reserve_in,
                reserve_out,
                s.fees,
                s.creator_fee_on.is_on_input(direction),
                s.version,
            )
            .map(|q| q.quote)
        }
    }
}

#[inline]
fn cashback_token(edge: &Edge, location: CashbackLocation) -> Option<u32> {
    match location {
        CashbackLocation::None => None,
        CashbackLocation::Input => Some(edge.from_token),
        CashbackLocation::Output => Some(edge.to_token),
    }
}

pub fn quote_cycle(
    cycle: &Cycle,
    pools: &[PoolCell],
    initial_amount: u64,
) -> Result<RouteResult, QuoteError> {
    let mut amount = initial_amount;
    let mut total_fee = 0u64;
    let mut base_cashback = 0u64;
    let mut unconverted_cashback_legs = 0u8;

    for edge in cycle.edge_iter() {
        let state = pools[edge.pool as usize].state.load();
        let q = quote_edge(state.as_ref(), edge.direction, amount)?;

        if q.cashback != 0 {
            match cashback_token(edge, q.cashback_location) {
                Some(token) if token == cycle.start_token => {
                    base_cashback = base_cashback.saturating_add(q.cashback);
                }
                Some(_) => {
                    unconverted_cashback_legs =
                        unconverted_cashback_legs.saturating_add(1);
                }
                None => {}
            }
        }

        amount = q.amount_out;
        total_fee = total_fee.saturating_add(q.dex_fee);
    }

    let gross_profit = amount as i128 - initial_amount as i128;
    let effective_profit = gross_profit + base_cashback as i128;

    Ok(RouteResult {
        amount_in: initial_amount,
        amount_out: amount,
        total_fee,
        base_cashback,
        unconverted_cashback_legs,
        gross_profit,
        effective_profit,
    })
}

pub fn marginal_candidate(
    cycle: &Cycle,
    pools: &[PoolCell],
    probe: u64,
    minimum_effective_profit: i128,
) -> bool {
    quote_cycle(cycle, pools, probe)
        .map(|q| q.effective_profit > minimum_effective_profit)
        .unwrap_or(false)
}
