use std::sync::Arc;

use smallvec::SmallVec;
use thiserror::Error;

use crate::{
    active_store::ActivePoolStore,
    graph::{Cycle, Edge},
    quote::{CashbackLocation, Quote, QuoteError},
    state::{PoolCell, PoolState},
    types::{Direction, PoolId, StateVersion},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuoteContext {
    pub current_timestamp: u64,
    pub current_slot: u64,
}

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SearchError {
    #[error(transparent)]
    Quote(#[from] QuoteError),
    #[error("cycle references missing pool {0}")]
    MissingPool(PoolId),
    #[error("pool {0} is not active")]
    InactivePool(PoolId),
    #[error(
        "cycle snapshot slot skew exceeds limit: min={min_slot} max={max_slot} allowed={max_allowed}"
    )]
    SlotSkew {
        min_slot: u64,
        max_slot: u64,
        max_allowed: u64,
    },
    #[error("pool {pool} changed during quote")]
    StaleGeneration {
        pool: PoolId,
        expected: StateVersion,
        actual: StateVersion,
    },
}

pub struct ActiveCycleSnapshot {
    pool_ids: SmallVec<[PoolId; 3]>,
    states: SmallVec<[Arc<PoolState>; 3]>,
}

impl ActiveCycleSnapshot {
    pub fn versions(&self) -> SmallVec<[StateVersion; 3]> {
        self.states.iter().map(|state| state.version()).collect()
    }

    pub fn quote(&self, cycle: &Cycle, amount_in: u64) -> Result<RouteResult, QuoteError> {
        quote_cycle_from_snapshots(cycle, &self.states, amount_in, None)
    }

    pub fn quote_at(
        &self,
        cycle: &Cycle,
        amount_in: u64,
        context: QuoteContext,
    ) -> Result<RouteResult, QuoteError> {
        quote_cycle_from_snapshots(cycle, &self.states, amount_in, Some(context))
    }

    pub fn max_slot(&self) -> u64 {
        self.states
            .iter()
            .map(|state| state.version().slot)
            .max()
            .unwrap_or(0)
    }

    pub fn validate(&self, store: &ActivePoolStore) -> Result<(), SearchError> {
        for (pool_id, captured) in self.pool_ids.iter().zip(self.states.iter()) {
            let current = store
                .get(*pool_id)
                .ok_or(SearchError::InactivePool(*pool_id))?
                .version();
            let expected = captured.version();

            if current != expected {
                return Err(SearchError::StaleGeneration {
                    pool: *pool_id,
                    expected,
                    actual: current,
                });
            }
        }

        Ok(())
    }
}

#[inline]
fn quote_edge(
    state: &PoolState,
    direction: Direction,
    amount_in: u64,
    context: Option<QuoteContext>,
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

        #[cfg(feature = "meteora-damm")]
        PoolState::MeteoraDamm(s) => {
            let context = context.ok_or(QuoteError::MeteoraDammQuote)?;
            crate::quote::meteora_damm::quote_exact_in_official(
                s.pool.as_ref(),
                amount_in,
                direction,
                context.current_timestamp,
                context.current_slot,
                s.version,
            )
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

fn quote_cycle_from_snapshots(
    cycle: &Cycle,
    snapshots: &[Arc<PoolState>],
    initial_amount: u64,
    context: Option<QuoteContext>,
) -> Result<RouteResult, QuoteError> {
    let mut amount = initial_amount;
    let mut total_fee = 0u64;
    let mut base_cashback = 0u64;
    let mut unconverted_cashback_legs = 0u8;

    for (edge, state) in cycle.edge_iter().zip(snapshots.iter()) {
        let q = quote_edge(state.as_ref(), edge.direction, amount, context)?;

        if q.cashback != 0 {
            match cashback_token(edge, q.cashback_location) {
                Some(token) if token == cycle.start_token => {
                    base_cashback = base_cashback.saturating_add(q.cashback);
                }
                Some(_) => {
                    unconverted_cashback_legs = unconverted_cashback_legs.saturating_add(1);
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

fn ensure_slot_skew(snapshots: &[Arc<PoolState>], max_slot_skew: u64) -> Result<(), SearchError> {
    if snapshots.is_empty() {
        return Ok(());
    }

    let mut min_slot = u64::MAX;
    let mut max_slot = 0u64;

    for state in snapshots {
        let slot = state.version().slot;
        min_slot = min_slot.min(slot);
        max_slot = max_slot.max(slot);
    }

    if max_slot.saturating_sub(min_slot) > max_slot_skew {
        return Err(SearchError::SlotSkew {
            min_slot,
            max_slot,
            max_allowed: max_slot_skew,
        });
    }

    Ok(())
}

fn load_cycle_snapshots(
    cycle: &Cycle,
    pools: &[PoolCell],
    max_slot_skew: u64,
) -> Result<SmallVec<[Arc<PoolState>; 3]>, SearchError> {
    let mut snapshots = SmallVec::<[Arc<PoolState>; 3]>::new();

    for edge in cycle.edge_iter() {
        let pool = pools
            .get(edge.pool as usize)
            .ok_or(SearchError::MissingPool(edge.pool))?;
        snapshots.push(pool.state.load_full());
    }

    ensure_slot_skew(&snapshots, max_slot_skew)?;
    Ok(snapshots)
}

pub fn capture_active_cycle(
    cycle: &Cycle,
    store: &ActivePoolStore,
    max_slot_skew: u64,
) -> Result<ActiveCycleSnapshot, SearchError> {
    let mut pool_ids = SmallVec::<[PoolId; 3]>::new();
    let mut states = SmallVec::<[Arc<PoolState>; 3]>::new();

    for edge in cycle.edge_iter() {
        let cell = store
            .get(edge.pool)
            .ok_or(SearchError::InactivePool(edge.pool))?;
        pool_ids.push(edge.pool);
        states.push(cell.state.load_full());
    }

    ensure_slot_skew(&states, max_slot_skew)?;
    Ok(ActiveCycleSnapshot { pool_ids, states })
}

pub fn quote_cycle(
    cycle: &Cycle,
    pools: &[PoolCell],
    initial_amount: u64,
) -> Result<RouteResult, QuoteError> {
    let mut snapshots = SmallVec::<[Arc<PoolState>; 3]>::new();

    for edge in cycle.edge_iter() {
        let state = pools[edge.pool as usize].state.load_full();
        snapshots.push(state);
    }

    quote_cycle_from_snapshots(cycle, &snapshots, initial_amount, None)
}

pub fn quote_cycle_at(
    cycle: &Cycle,
    pools: &[PoolCell],
    initial_amount: u64,
    context: QuoteContext,
) -> Result<RouteResult, QuoteError> {
    let mut snapshots = SmallVec::<[Arc<PoolState>; 3]>::new();

    for edge in cycle.edge_iter() {
        let state = pools[edge.pool as usize].state.load_full();
        snapshots.push(state);
    }

    quote_cycle_from_snapshots(cycle, &snapshots, initial_amount, Some(context))
}

pub fn quote_cycle_consistent(
    cycle: &Cycle,
    pools: &[PoolCell],
    initial_amount: u64,
    max_slot_skew: u64,
) -> Result<RouteResult, SearchError> {
    let snapshots = load_cycle_snapshots(cycle, pools, max_slot_skew)?;
    let result = quote_cycle_from_snapshots(cycle, &snapshots, initial_amount, None)?;

    for (edge, captured) in cycle.edge_iter().zip(snapshots.iter()) {
        let current = pools
            .get(edge.pool as usize)
            .ok_or(SearchError::MissingPool(edge.pool))?
            .version();
        let expected = captured.version();

        if current != expected {
            return Err(SearchError::StaleGeneration {
                pool: edge.pool,
                expected,
                actual: current,
            });
        }
    }

    Ok(result)
}

pub fn quote_active_cycle_consistent(
    cycle: &Cycle,
    store: &ActivePoolStore,
    initial_amount: u64,
    max_slot_skew: u64,
) -> Result<RouteResult, SearchError> {
    let snapshot = capture_active_cycle(cycle, store, max_slot_skew)?;
    let result = snapshot.quote(cycle, initial_amount)?;
    snapshot.validate(store)?;
    Ok(result)
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

pub fn marginal_candidate_active(
    cycle: &Cycle,
    store: &ActivePoolStore,
    probe: u64,
    minimum_effective_profit: i128,
    max_slot_skew: u64,
) -> bool {
    quote_active_cycle_consistent(cycle, store, probe, max_slot_skew)
        .map(|q| q.effective_profit > minimum_effective_profit)
        .unwrap_or(false)
}
