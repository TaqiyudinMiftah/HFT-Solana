use crate::{
    active_store::ActivePoolStore,
    graph::Cycle,
    search::{capture_active_cycle, quote_cycle, RouteResult, SearchError},
    state::PoolCell,
};

/// Cheap V1 optimizer around a historical/heuristic seed.
///
/// The production optimizer will bracket and refine the local maximum and
/// use a separate bin-aware path for DLMM routes.
pub fn optimize_size(
    cycle: &Cycle,
    pools: &[PoolCell],
    seed: u64,
    max_size: u64,
) -> Option<RouteResult> {
    if seed == 0 || max_size == 0 {
        return None;
    }

    let probes = [
        seed / 4,
        seed / 2,
        seed,
        seed.saturating_mul(2),
        seed.saturating_mul(4),
    ];

    let mut best: Option<RouteResult> = None;

    for amount in probes {
        if amount == 0 || amount > max_size {
            continue;
        }

        let Ok(result) = quote_cycle(cycle, pools, amount) else {
            continue;
        };

        if best
            .as_ref()
            .map(|current| result.effective_profit > current.effective_profit)
            .unwrap_or(true)
        {
            best = Some(result);
        }
    }

    best
}

/// Active-store optimizer that captures one immutable cycle snapshot and uses
/// it for every size probe, then revalidates pool generations once at the end.
pub fn optimize_size_active(
    cycle: &Cycle,
    store: &ActivePoolStore,
    seed: u64,
    max_size: u64,
    max_slot_skew: u64,
) -> Result<Option<RouteResult>, SearchError> {
    if seed == 0 || max_size == 0 {
        return Ok(None);
    }

    let snapshot = capture_active_cycle(cycle, store, max_slot_skew)?;
    let probes = [
        seed / 4,
        seed / 2,
        seed,
        seed.saturating_mul(2),
        seed.saturating_mul(4),
    ];

    let mut best: Option<RouteResult> = None;

    for amount in probes {
        if amount == 0 || amount > max_size {
            continue;
        }

        let result = snapshot.quote(cycle, amount)?;
        if best
            .as_ref()
            .map(|current| result.effective_profit > current.effective_profit)
            .unwrap_or(true)
        {
            best = Some(result);
        }
    }

    snapshot.validate(store)?;
    Ok(best)
}
