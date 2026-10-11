use crate::landing::{LandingCandidate, LandingProvider, BPS_DENOMINATOR};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbabilityPrior {
    pub successes: u64,
    pub failures: u64,
}

impl ProbabilityPrior {
    pub const fn laplace() -> Self {
        Self {
            successes: 1,
            failures: 1,
        }
    }

    pub fn is_valid(self) -> bool {
        self.successes.saturating_add(self.failures) > 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProviderOutcomeCounts {
    pub successes: u64,
    pub failures: u64,
}

impl ProviderOutcomeCounts {
    pub fn samples(self) -> u64 {
        self.successes.saturating_add(self.failures)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LandingOutcome {
    pub provider: LandingProvider,
    /// Success means the delivery path landed and the arb committed.
    pub success: bool,
    /// Integer replay weight. Zero-weight observations are ignored.
    pub weight: u64,
}

impl LandingOutcome {
    pub const fn single(provider: LandingProvider, success: bool) -> Self {
        Self {
            provider,
            success,
            weight: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LandingProbabilityCalibrator {
    prior: ProbabilityPrior,
    direct: ProviderOutcomeCounts,
    jito: ProviderOutcomeCounts,
    helius_sender: ProviderOutcomeCounts,
}

impl LandingProbabilityCalibrator {
    pub fn new(prior: ProbabilityPrior) -> Option<Self> {
        prior.is_valid().then_some(Self {
            prior,
            direct: ProviderOutcomeCounts::default(),
            jito: ProviderOutcomeCounts::default(),
            helius_sender: ProviderOutcomeCounts::default(),
        })
    }

    pub fn with_laplace_prior() -> Self {
        Self::new(ProbabilityPrior::laplace()).expect("Laplace prior is valid")
    }

    pub fn prior(&self) -> ProbabilityPrior {
        self.prior
    }

    pub fn observe(&mut self, outcome: LandingOutcome) {
        if outcome.weight == 0 {
            return;
        }

        let counts = self.counts_mut(outcome.provider);
        if outcome.success {
            counts.successes = counts.successes.saturating_add(outcome.weight);
        } else {
            counts.failures = counts.failures.saturating_add(outcome.weight);
        }
    }

    pub fn observe_many(&mut self, outcomes: impl IntoIterator<Item = LandingOutcome>) {
        for outcome in outcomes {
            self.observe(outcome);
        }
    }

    pub fn counts(&self, provider: LandingProvider) -> ProviderOutcomeCounts {
        match provider {
            LandingProvider::Direct => self.direct,
            LandingProvider::Jito => self.jito,
            LandingProvider::HeliusSender => self.helius_sender,
        }
    }

    pub fn estimate_success_probability_bps(&self, provider: LandingProvider) -> u16 {
        let counts = self.counts(provider);
        let successes = counts.successes.saturating_add(self.prior.successes);
        let total = counts
            .samples()
            .saturating_add(self.prior.successes)
            .saturating_add(self.prior.failures);

        if total == 0 {
            return 0;
        }

        let numerator = (successes as u128).saturating_mul(BPS_DENOMINATOR as u128);
        let rounded = numerator
            .saturating_add((total / 2) as u128)
            .checked_div(total as u128)
            .unwrap_or(0)
            .min(BPS_DENOMINATOR as u128);

        rounded as u16
    }

    pub fn calibrate_candidate(&self, mut candidate: LandingCandidate) -> LandingCandidate {
        candidate.success_probability_bps =
            self.estimate_success_probability_bps(candidate.provider);
        candidate
    }

    fn counts_mut(&mut self, provider: LandingProvider) -> &mut ProviderOutcomeCounts {
        match provider {
            LandingProvider::Direct => &mut self.direct,
            LandingProvider::Jito => &mut self.jito,
            LandingProvider::HeliusSender => &mut self.helius_sender,
        }
    }
}
