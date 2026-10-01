use crate::{
    graph::Cycle,
    quote::{cpmm::quote_xyk_exact_in, Quote, QuoteError},
    state::{PoolCell, PoolState},
    types::Direction,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteResult {
    pub amount_in: u64,
    pub amount_out: u64,
    pub total_fee: u64,
    pub cashback: u64,
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
        PoolState::Pump(s) => {
            let (reserve_in, reserve_out) = match direction {
                Direction::AtoB => (s.reserve_a, s.reserve_b),
                Direction::BtoA => (s.reserve_b, s.reserve_a),
            };

            let mut q =
                quote_xyk_exact_in(amount_in, reserve_in, reserve_out, s.fee_ppm, s.version)?;

            // Research placeholder. Replace with exact Pump cashback semantics.
            q.cashback = u64::try_from(
                (amount_in as u128)
                    .checked_mul(s.cashback_ppm as u128)
                    .ok_or(QuoteError::MathOverflow)?
                    / 1_000_000u128,
            )
            .map_err(|_| QuoteError::MathOverflow)?;

            Ok(q)
        }
        PoolState::RaydiumCpmm(s) | PoolState::MeteoraDamm(s) => {
            let (reserve_in, reserve_out) = match direction {
                Direction::AtoB => (s.reserve_a, s.reserve_b),
                Direction::BtoA => (s.reserve_b, s.reserve_a),
            };

            quote_xyk_exact_in(amount_in, reserve_in, reserve_out, s.fee_ppm, s.version)
        }
    }
}

pub fn quote_cycle(
    cycle: &Cycle,
    pools: &[PoolCell],
    initial_amount: u64,
) -> Result<RouteResult, QuoteError> {
    let mut amount = initial_amount;
    let mut total_fee = 0u64;
    let mut cashback = 0u64;

    for edge in cycle.edge_iter() {
        let state = pools[edge.pool as usize].state.load();
        let q = quote_edge(state.as_ref(), edge.direction, amount)?;

        amount = q.amount_out;
        total_fee = total_fee.saturating_add(q.dex_fee);
        cashback = cashback.saturating_add(q.cashback);
    }

    let gross_profit = amount as i128 - initial_amount as i128;
    let effective_profit = gross_profit + cashback as i128;

    Ok(RouteResult {
        amount_in: initial_amount,
        amount_out: amount,
        total_fee,
        cashback,
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
