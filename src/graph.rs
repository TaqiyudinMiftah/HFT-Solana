use smallvec::SmallVec;

use crate::types::{CycleId, Direction, PoolId, TokenId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub pool: PoolId,
    pub direction: Direction,
    pub from_token: TokenId,
    pub to_token: TokenId,
}

#[derive(Clone, Copy, Debug)]
pub struct Cycle {
    pub id: CycleId,
    pub len: u8,
    pub edges: [Edge; 3],
    pub start_token: TokenId,
}

impl Cycle {
    pub fn edge_iter(&self) -> impl Iterator<Item = &Edge> {
        self.edges[..self.len as usize].iter()
    }
}

#[derive(Default)]
pub struct GraphIndex {
    pub cycles: Vec<Cycle>,
    pub pool_to_cycles: Vec<Box<[CycleId]>>,
}

impl GraphIndex {
    pub fn from_cycles(pool_count: usize, cycles: Vec<Cycle>) -> Self {
        let mut pool_to_cycles = vec![Vec::<CycleId>::new(); pool_count];

        for (index, cycle) in cycles.iter().enumerate() {
            assert_eq!(
                cycle.id as usize, index,
                "cycle ids must be dense and match vector index"
            );

            let mut seen = SmallVec::<[PoolId; 3]>::new();
            for edge in cycle.edge_iter() {
                assert!(
                    (edge.pool as usize) < pool_count,
                    "cycle references pool outside graph"
                );

                if !seen.contains(&edge.pool) {
                    seen.push(edge.pool);
                    pool_to_cycles[edge.pool as usize].push(cycle.id);
                }
            }
        }

        Self {
            cycles,
            pool_to_cycles: pool_to_cycles
                .into_iter()
                .map(Vec::into_boxed_slice)
                .collect(),
        }
    }
}
