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
