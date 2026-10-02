use smallvec::SmallVec;

use crate::{
    active_store::ActivePoolStore,
    graph::Cycle,
    search::{capture_active_cycle, quote_cycle, RouteResult, SearchError},
    state::PoolCell,
};

const MAX_GEOMETRIC_STEPS: usize = 6;
const MAX_REFINE_STEPS: usize = 10;

#[inline]
fn consider(best: &mut Option<RouteResult>, candidate: Option<RouteResult>) {
    let Some(candidate) = candidate else {
        return;
    };

    let improve = best
        .as_ref()
        .map(|current| candidate.effective_profit > current.effective_profit)
        .unwrap_or(true);

    if improve {
        *best = Some(candidate);
    }
}

/// Bounded numeric optimizer for CPMM-like routes.
///
/// Stage 1 evaluates a small geometric ladder around the historical seed to
/// bracket the profitable region. Stage 2 applies integer ternary refinement
/// inside the neighboring anchors around the best ladder point.
///
/// DEX-specific DLMM/bin routes will use a separate optimizer later.
fn optimize_with<F>(seed: u64, min_size: u64, max_size: u64, mut quote: F) -> Option<RouteResult>
where
    F: FnMut(u64) -> Option<RouteResult>,
{
    if min_size == 0 || max_size < min_size {
        return None;
    }

    let seed = seed.clamp(min_size, max_size);
    let mut anchors = SmallVec::<[u64; 16]>::new();
    anchors.push(min_size);
    anchors.push(seed);
    anchors.push(max_size);

    let mut lower = seed;
    for _ in 0..MAX_GEOMETRIC_STEPS {
        let next = (lower / 2).max(min_size);
        if next == lower {
            break;
        }
        anchors.push(next);
        lower = next;
        if lower == min_size {
            break;
        }
    }

    let mut upper = seed;
    for _ in 0..MAX_GEOMETRIC_STEPS {
        let next = upper.saturating_mul(2).min(max_size);
        if next == upper {
            break;
        }
        anchors.push(next);
        upper = next;
        if upper == max_size {
            break;
        }
    }

    anchors.sort_unstable();
    anchors.dedup();

    let mut best: Option<RouteResult> = None;
    for &amount in &anchors {
        consider(&mut best, quote(amount));
    }

    let best_amount = best.as_ref()?.amount_in;
    let best_index = anchors
        .iter()
        .position(|amount| *amount == best_amount)
        .unwrap_or_else(|| {
            anchors
                .partition_point(|amount| *amount < best_amount)
                .min(anchors.len().saturating_sub(1))
        });

    let mut left = anchors[best_index.saturating_sub(1)];
    let mut right = anchors[(best_index + 1).min(anchors.len() - 1)];

    for _ in 0..MAX_REFINE_STEPS {
        if right <= left.saturating_add(3) {
            break;
        }

        let width = right - left;
        let third = (width / 3).max(1);
        let m1 = left.saturating_add(third);
        let m2 = right.saturating_sub(third);

        if m1 >= m2 {
            break;
        }

        let r1 = quote(m1);
        let r2 = quote(m2);
        let p1 = r1
            .as_ref()
            .map(|result| result.effective_profit)
            .unwrap_or(i128::MIN);
        let p2 = r2
            .as_ref()
            .map(|result| result.effective_profit)
            .unwrap_or(i128::MIN);

        consider(&mut best, r1);
        consider(&mut best, r2);

        if p1 < p2 {
            left = m1.saturating_add(1);
        } else {
            right = m2.saturating_sub(1);
        }
    }

    let midpoint = left.saturating_add((right - left) / 2);
    for amount in [
        left,
        midpoint,
        right,
        best.as_ref()?.amount_in.saturating_sub(1).max(min_size),
        best.as_ref()?.amount_in.saturating_add(1).min(max_size),
    ] {
        consider(&mut best, quote(amount));
    }

    best
}

/// Backward-compatible optimizer with a minimum raw input of one unit.
pub fn optimize_size(
    cycle: &Cycle,
    pools: &[PoolCell],
    seed: u64,
    max_size: u64,
) -> Option<RouteResult> {
    optimize_size_bounded(cycle, pools, seed, 1, max_size)
}

pub fn optimize_size_bounded(
    cycle: &Cycle,
    pools: &[PoolCell],
    seed: u64,
    min_size: u64,
    max_size: u64,
) -> Option<RouteResult> {
    optimize_with(seed, min_size, max_size, |amount| {
        quote_cycle(cycle, pools, amount).ok()
    })
}

pub fn optimize_size_on_snapshot(
    cycle: &Cycle,
    snapshot: &crate::search::ActiveCycleSnapshot,
    seed: u64,
    max_size: u64,
) -> Option<RouteResult> {
    optimize_size_on_snapshot_bounded(cycle, snapshot, seed, 1, max_size)
}

pub fn optimize_size_on_snapshot_bounded(
    cycle: &Cycle,
    snapshot: &crate::search::ActiveCycleSnapshot,
    seed: u64,
    min_size: u64,
    max_size: u64,
) -> Option<RouteResult> {
    optimize_with(seed, min_size, max_size, |amount| {
        snapshot.quote(cycle, amount).ok()
    })
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
    optimize_size_active_bounded(cycle, store, seed, 1, max_size, max_slot_skew)
}

pub fn optimize_size_active_bounded(
    cycle: &Cycle,
    store: &ActivePoolStore,
    seed: u64,
    min_size: u64,
    max_size: u64,
    max_slot_skew: u64,
) -> Result<Option<RouteResult>, SearchError> {
    if min_size == 0 || max_size < min_size {
        return Ok(None);
    }

    let snapshot = capture_active_cycle(cycle, store, max_slot_skew)?;
    let best = optimize_size_on_snapshot_bounded(cycle, &snapshot, seed, min_size, max_size);
    snapshot.validate(store)?;
    Ok(best)
}
