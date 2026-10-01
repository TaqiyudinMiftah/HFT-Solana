use crate::{
    graph::Cycle,
    search::{quote_cycle, RouteResult},
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
