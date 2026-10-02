use smallvec::SmallVec;

use crate::{
    active_store::ActivePoolStore,
    graph::GraphIndex,
    opportunity::Opportunity,
    search::capture_active_cycle,
    sizing::optimize_size_on_snapshot_bounded,
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

pub struct OpportunityEngine {
    graph: GraphIndex,
    runtime: Vec<CycleRuntime>,
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

        Self { graph, runtime }
    }

    pub fn graph(&self) -> &GraphIndex {
        &self.graph
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

        let snapshot = capture_active_cycle(cycle, store, runtime.config.max_slot_skew).ok()?;
        let versions = snapshot.versions();

        if runtime.last_evaluated == versions {
            return None;
        }
        runtime.last_evaluated = versions;

        let probe = snapshot.quote(cycle, runtime.config.minimum_probe).ok()?;
        if probe.effective_profit <= 0 {
            return None;
        }

        let seed = runtime.last_q_star.max(runtime.config.minimum_probe);
        let best = optimize_size_on_snapshot_bounded(
            cycle,
            &snapshot,
            seed,
            runtime.config.minimum_probe,
            runtime.config.max_size,
        )?;

        if best.effective_profit < runtime.config.minimum_effective_profit {
            return None;
        }

        snapshot.validate(store).ok()?;
        runtime.last_q_star = best.amount_in;

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
