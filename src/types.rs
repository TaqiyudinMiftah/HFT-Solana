pub type PoolId = u32;
pub type TokenId = u32;
pub type CycleId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateVersion {
    pub slot: u64,
    pub write_version: u64,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DexKind {
    PumpSwap,
    RaydiumCpmm,
    MeteoraDammV2,
    MeteoraDlmm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    AtoB,
    BtoA,
}
