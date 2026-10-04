use crate::opportunity::Opportunity;

pub const BPS_DENOMINATOR: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LandingProvider {
    Direct,
    Jito,
    HeliusSender,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LandingCandidate {
    pub provider: LandingProvider,
    /// Estimated probability that this delivery path lands and the arb commits.
    pub success_probability_bps: u16,
    /// Priority fee paid on a successful transaction.
    pub priority_fee: u64,
    /// Relay/provider tip. Atomic tip transfers are modeled as success-only.
    pub relay_tip: u64,
    /// Total fee/cost retained by the network when the attempt lands but the
    /// on-chain arb guard reverts. Relay tips are assumed to revert atomically.
    pub failure_fee: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LandingPolicyConfig {
    /// Normal transaction/base fee included on successful execution.
    pub base_fee: u64,
    /// Minimum retained profit when the arb commits.
    pub minimum_net_if_landed: i128,
    /// Minimum probability-weighted EV required by the paper policy.
    pub minimum_expected_value: i128,
    /// Hard relay-tip cap as a share of expected effective profit.
    pub max_tip_share_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LandingChoice {
    pub provider: LandingProvider,
    pub success_probability_bps: u16,
    pub priority_fee: u64,
    pub relay_tip: u64,
    pub net_if_landed: i128,
    pub expected_value: i128,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LandingPaperStats {
    pub evaluated: u64,
    pub selected: u64,
    pub skipped: u64,
    pub direct: u64,
    pub jito: u64,
    pub helius_sender: u64,
    pub expected_value_total: i128,
}

impl LandingPaperStats {
    pub fn record_choice(&mut self, choice: LandingChoice) {
        self.evaluated = self.evaluated.saturating_add(1);
        self.selected = self.selected.saturating_add(1);
        self.expected_value_total = self.expected_value_total.saturating_add(choice.expected_value);

        match choice.provider {
            LandingProvider::Direct => self.direct = self.direct.saturating_add(1),
            LandingProvider::Jito => self.jito = self.jito.saturating_add(1),
            LandingProvider::HeliusSender => {
                self.helius_sender = self.helius_sender.saturating_add(1)
            }
        }
    }

    pub fn record_skip(&mut self) {
        self.evaluated = self.evaluated.saturating_add(1);
        self.skipped = self.skipped.saturating_add(1);
    }
}

fn tip_share_allowed(effective_profit: i128, relay_tip: u64, max_tip_share_bps: u16) -> bool {
    if effective_profit <= 0 || max_tip_share_bps as u64 > BPS_DENOMINATOR {
        return false;
    }

    (relay_tip as u128)
        .checked_mul(BPS_DENOMINATOR as u128)
        .is_some_and(|lhs| {
            lhs <= (effective_profit as u128).saturating_mul(max_tip_share_bps as u128)
        })
}

pub fn evaluate_landing_candidate(
    opportunity: &Opportunity,
    candidate: LandingCandidate,
    config: LandingPolicyConfig,
) -> Option<LandingChoice> {
    if candidate.success_probability_bps as u64 > BPS_DENOMINATOR {
        return None;
    }
    if !tip_share_allowed(
        opportunity.expected_effective_profit,
        candidate.relay_tip,
        config.max_tip_share_bps,
    ) {
        return None;
    }

    let success_cost = config
        .base_fee
        .saturating_add(candidate.priority_fee)
        .saturating_add(candidate.relay_tip);
    let net_if_landed = opportunity.expected_effective_profit - success_cost as i128;
    if net_if_landed < config.minimum_net_if_landed {
        return None;
    }

    let success_bps = candidate.success_probability_bps as i128;
    let failure_bps = BPS_DENOMINATOR as i128 - success_bps;
    let ev_numerator = success_bps
        .checked_mul(net_if_landed)?
        .checked_sub(failure_bps.checked_mul(candidate.failure_fee as i128)?)?;
    let expected_value = ev_numerator / BPS_DENOMINATOR as i128;

    (expected_value >= config.minimum_expected_value).then_some(LandingChoice {
        provider: candidate.provider,
        success_probability_bps: candidate.success_probability_bps,
        priority_fee: candidate.priority_fee,
        relay_tip: candidate.relay_tip,
        net_if_landed,
        expected_value,
    })
}

/// Choose the candidate with the highest expected value.
///
/// Ties prefer higher landing probability, then lower success cost, then the
/// lowest enum value for deterministic historical replay.
pub fn choose_landing_path(
    opportunity: &Opportunity,
    candidates: impl IntoIterator<Item = LandingCandidate>,
    config: LandingPolicyConfig,
) -> Option<LandingChoice> {
    candidates
        .into_iter()
        .filter_map(|candidate| evaluate_landing_candidate(opportunity, candidate, config))
        .max_by(|left, right| {
            left.expected_value
                .cmp(&right.expected_value)
                .then_with(|| {
                    left.success_probability_bps
                        .cmp(&right.success_probability_bps)
                })
                .then_with(|| {
                    let left_cost = left.priority_fee.saturating_add(left.relay_tip);
                    let right_cost = right.priority_fee.saturating_add(right.relay_tip);
                    right_cost.cmp(&left_cost)
                })
                .then_with(|| right.provider.cmp(&left.provider))
        })
}
