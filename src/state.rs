use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::{
    quote::{pump::PumpFeesBps, raydium::RaydiumFees},
    types::{Direction, StateVersion},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatorFeeOn {
    Both,
    OnlyA,
    OnlyB,
}

impl CreatorFeeOn {
    pub fn is_on_input(self, direction: Direction) -> bool {
        match (self, direction) {
            (CreatorFeeOn::Both, _) => true,
            (CreatorFeeOn::OnlyA, Direction::AtoB) => true,
            (CreatorFeeOn::OnlyA, Direction::BtoA) => false,
            (CreatorFeeOn::OnlyB, Direction::AtoB) => false,
            (CreatorFeeOn::OnlyB, Direction::BtoA) => true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RaydiumCpmmState {
    pub version: StateVersion,
    pub reserve_a: u64,
    pub reserve_b: u64,
    pub fees: RaydiumFees,
    pub creator_fee_on: CreatorFeeOn,
}

#[cfg(feature = "meteora-damm")]
#[derive(Clone, Debug)]
pub struct MeteoraDammState {
    pub version: StateVersion,
    pub pool: Arc<meteora_cp_amm::state::Pool>,
}

#[derive(Clone, Debug)]
pub struct PumpState {
    pub version: StateVersion,

    /// A is base token, B is quote token.
    pub base_reserve: u64,
    pub raw_quote_reserve: u64,
    pub virtual_quote_reserves: i128,

    /// Resolved for the current pool state/market-cap tier.
    pub fees: PumpFeesBps,
    pub cashback_coin: bool,
}

#[derive(Clone, Debug)]
pub enum PoolState {
    Pump(PumpState),
    RaydiumCpmm(RaydiumCpmmState),
    #[cfg(feature = "meteora-damm")]
    MeteoraDamm(MeteoraDammState),
}

impl PoolState {
    #[inline(always)]
    pub fn version(&self) -> StateVersion {
        match self {
            PoolState::Pump(state) => state.version,
            PoolState::RaydiumCpmm(state) => state.version,
            #[cfg(feature = "meteora-damm")]
            PoolState::MeteoraDamm(state) => state.version,
        }
    }
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

    #[inline(always)]
    pub fn version(&self) -> StateVersion {
        self.state.load().version()
    }
}
