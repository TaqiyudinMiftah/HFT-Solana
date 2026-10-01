use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::types::StateVersion;

#[derive(Clone, Debug)]
pub struct CpmmState {
    pub version: StateVersion,
    pub reserve_a: u64,
    pub reserve_b: u64,
    pub fee_ppm: u64,
}

#[derive(Clone, Debug)]
pub struct PumpState {
    pub version: StateVersion,
    pub reserve_a: u64,
    pub reserve_b: u64,
    pub fee_ppm: u64,

    /// Research placeholder only. Production cashback logic must match
    /// current Pump program/config integer semantics exactly.
    pub cashback_ppm: u64,
}

#[derive(Clone, Debug)]
pub enum PoolState {
    Pump(PumpState),
    RaydiumCpmm(CpmmState),
    MeteoraDamm(CpmmState),
}

pub struct PoolCell {
    pub state: ArcSwap<PoolState>,
}

impl PoolCell {
    pub fn new(initial: PoolState) -> Self {
        Self {
            state: ArcSwap::new(Arc::new(initial)),
        }
    }

    pub fn replace(&self, next: PoolState) {
        self.state.store(Arc::new(next));
    }
}
