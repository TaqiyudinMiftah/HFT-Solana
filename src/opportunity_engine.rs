use smallvec::SmallVec;

use crate::{
    active_store::ActivePoolStore,
    graph::GraphIndex,
    opportunity::Opportunity,
    search::{capture_active_cycle, QuoteContext},
    sizing::optimize_size_on_snapshot_bounded_at,
    types::{CycleId, PoolId, StateVersion},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CycleSearchConfig {
    pub initial_seed: u64,
    pub minimum_probe: u64,
    pub max_size: u64,
    pub minimum_effective_profit: i128,
    pub max_slot_skew: u64,
    pub expected_cu: u32,
}

#[derive(Clone, Debug)]
struct CycleRuntime {
    last_q_star: u64,
    last_evaluated: SmallVec<[StateVersion; 3]>,
    config: CycleSearchConfig,
}

/// Cumulative decisions for distinct cycle state vectors. Rejections partition
/// `evaluated`; unavailable and duplicate snapshots are counted separately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStats {
    pub evaluated: u64,
    pub snapshot_unavailable: u64,
    pub duplicate_snapshot: u64,
    pub probe_quote_failed: u64,
    pub nonpositive_probe: u64,
    pub sizing_failed: u64,
    pub below_minimum_profit: u64,
    pub stale_snapshot: u64,
    pub opportunities: u64,
}

pub struct OpportunityEngine {
    graph: GraphIndex,
    runtime: Vec<CycleRuntime>,
    stats: SearchStats,
}

impl OpportunityEngine {
    pub fn new(graph: GraphIndex, configs: Vec<CycleSearchConfig>) -> Self {
        assert_eq!(
            graph.cycles.len(),
            configs.len(),
            "each cycle needs one search config"
        );

        let runtime = configs
            .into_iter()
            .map(|config| CycleRuntime {
                last_q_star: config.initial_seed.max(config.minimum_probe),
                last_evaluated: SmallVec::new(),
                config,
            })
            .collect();

        Self {
            graph,
            runtime,
            stats: SearchStats::default(),
        }
    }

    pub fn graph(&self) -> &GraphIndex {
        &self.graph
    }

    pub fn stats(&self) -> SearchStats {
        self.stats
    }

    pub fn process_pool(
        &mut self,
        changed_pool: PoolId,
        store: &ActivePoolStore,
        created_ns: u64,
    ) -> Vec<Opportunity> {
        let Some(cycle_ids) = self.graph.pool_to_cycles.get(changed_pool as usize) else {
            return Vec::new();
        };
        let cycle_ids = SmallVec::<[CycleId; 8]>::from_slice(cycle_ids);

        let mut opportunities = Vec::new();

        for cycle_id in cycle_ids {
            if let Some(opportunity) = self.evaluate_cycle(cycle_id, store, created_ns) {
                opportunities.push(opportunity);
            }
        }

        opportunities
    }

    fn evaluate_cycle(
        &mut self,
        cycle_id: CycleId,
        store: &ActivePoolStore,
        created_ns: u64,
    ) -> Option<Opportunity> {
        let cycle = self.graph.cycles.get(cycle_id as usize)?;
        let runtime = self.runtime.get_mut(cycle_id as usize)?;

        let snapshot = match capture_active_cycle(cycle, store, runtime.config.max_slot_skew) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                self.stats.snapshot_unavailable = self.stats.snapshot_unavailable.saturating_add(1);
                return None;
            }
        };
        let versions = snapshot.versions();

        if runtime.last_evaluated == versions {
            self.stats.duplicate_snapshot = self.stats.duplicate_snapshot.saturating_add(1);
            return None;
        }
        runtime.last_evaluated = versions;
        self.stats.evaluated = self.stats.evaluated.saturating_add(1);

        let context = QuoteContext {
            current_timestamp: created_ns / 1_000_000_000,
            current_slot: snapshot.max_slot(),
        };

        let probe = match snapshot.quote_at(cycle, runtime.config.minimum_probe, context) {
            Ok(probe) => probe,
            Err(_) => {
                self.stats.probe_quote_failed = self.stats.probe_quote_failed.saturating_add(1);
                return None;
            }
        };
        if probe.effective_profit <= 0 {
            self.stats.nonpositive_probe = self.stats.nonpositive_probe.saturating_add(1);
            return None;
        }

        let seed = runtime.last_q_star.max(runtime.config.minimum_probe);
        let best = match optimize_size_on_snapshot_bounded_at(
            cycle,
            &snapshot,
            seed,
            runtime.config.minimum_probe,
            runtime.config.max_size,
            context,
        ) {
            Some(best) => best,
            None => {
                self.stats.sizing_failed = self.stats.sizing_failed.saturating_add(1);
                return None;
            }
        };

        if best.effective_profit < runtime.config.minimum_effective_profit {
            self.stats.below_minimum_profit = self.stats.below_minimum_profit.saturating_add(1);
            return None;
        }

        if snapshot.validate(store).is_err() {
            self.stats.stale_snapshot = self.stats.stale_snapshot.saturating_add(1);
            return None;
        }
        runtime.last_q_star = best.amount_in;
        self.stats.opportunities = self.stats.opportunities.saturating_add(1);

        Some(Opportunity {
            cycle_id,
            amount_in: best.amount_in,
            expected_out: best.amount_out,
            expected_effective_profit: best.effective_profit,
            expected_cu: runtime.config.expected_cu,
            created_ns,
        })
    }
}
