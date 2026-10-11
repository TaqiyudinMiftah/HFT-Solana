use crate::types::CycleId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opportunity {
    pub cycle_id: CycleId,
    pub amount_in: u64,
    pub expected_out: u64,
    pub expected_effective_profit: i128,
    pub expected_cu: u32,
    pub created_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LandingDecision {
    pub priority_fee: u64,
    pub relay_tip: u64,
    pub expected_net: i128,
}

pub fn evaluate_execution_cost(
    opportunity: &Opportunity,
    estimated_priority_fee: u64,
    relay_tip: u64,
    base_fee: u64,
    minimum_net: i128,
) -> Option<LandingDecision> {
    let expected_net = opportunity.expected_effective_profit
        - estimated_priority_fee as i128
        - relay_tip as i128
        - base_fee as i128;

    (expected_net >= minimum_net).then_some(LandingDecision {
        priority_fee: estimated_priority_fee,
        relay_tip,
        expected_net,
    })
}
