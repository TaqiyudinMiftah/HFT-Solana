use hft_solana::{
    landing::{LandingCandidate, LandingProvider},
    landing_calibration::{
        LandingOutcome, LandingProbabilityCalibrator, ProbabilityPrior, ProviderOutcomeCounts,
    },
};

fn candidate(provider: LandingProvider) -> LandingCandidate {
    LandingCandidate {
        provider,
        success_probability_bps: 1,
        priority_fee: 5_000,
        relay_tip: 0,
        relay_tip_share_bps: None,
        minimum_relay_tip: 0,
        maximum_relay_tip: None,
        failure_fee: 10_000,
    }
}

#[test]
fn laplace_prior_starts_at_fifty_percent() {
    let calibrator = LandingProbabilityCalibrator::with_laplace_prior();

    for provider in [
        LandingProvider::Direct,
        LandingProvider::Jito,
        LandingProvider::HeliusSender,
    ] {
        assert_eq!(calibrator.estimate_success_probability_bps(provider), 5_000);
    }
}

#[test]
fn observations_are_provider_isolated_and_weighted() {
    let mut calibrator = LandingProbabilityCalibrator::with_laplace_prior();
    calibrator.observe_many([
        LandingOutcome {
            provider: LandingProvider::Jito,
            success: true,
            weight: 3,
        },
        LandingOutcome::single(LandingProvider::Jito, false),
        LandingOutcome::single(LandingProvider::HeliusSender, true),
    ]);

    assert_eq!(
        calibrator.counts(LandingProvider::Jito),
        ProviderOutcomeCounts {
            successes: 3,
            failures: 1,
        }
    );
    assert_eq!(
        calibrator.counts(LandingProvider::HeliusSender),
        ProviderOutcomeCounts {
            successes: 1,
            failures: 0,
        }
    );
    assert_eq!(calibrator.counts(LandingProvider::Direct).samples(), 0);

    // Jito posterior: (3 + 1) / (3 + 1 + 1 + 1) = 4/6.
    assert_eq!(
        calibrator.estimate_success_probability_bps(LandingProvider::Jito),
        6_667
    );
    // Helius posterior: (1 + 1) / (1 + 1 + 1) = 2/3.
    assert_eq!(
        calibrator.estimate_success_probability_bps(LandingProvider::HeliusSender),
        6_667
    );
    assert_eq!(
        calibrator.estimate_success_probability_bps(LandingProvider::Direct),
        5_000
    );
}

#[test]
fn zero_weight_is_ignored_and_invalid_prior_is_rejected() {
    assert!(LandingProbabilityCalibrator::new(ProbabilityPrior {
        successes: 0,
        failures: 0,
    })
    .is_none());

    let mut calibrator = LandingProbabilityCalibrator::with_laplace_prior();
    calibrator.observe(LandingOutcome {
        provider: LandingProvider::Jito,
        success: true,
        weight: 0,
    });

    assert_eq!(calibrator.counts(LandingProvider::Jito).samples(), 0);
}

#[test]
fn calibration_changes_only_probability() {
    let mut calibrator = LandingProbabilityCalibrator::with_laplace_prior();
    calibrator.observe_many([
        LandingOutcome::single(LandingProvider::Jito, true),
        LandingOutcome::single(LandingProvider::Jito, true),
        LandingOutcome::single(LandingProvider::Jito, false),
    ]);

    let original = candidate(LandingProvider::Jito);
    let calibrated = calibrator.calibrate_candidate(original);

    assert_eq!(calibrated.success_probability_bps, 6_000);
    assert_eq!(calibrated.priority_fee, original.priority_fee);
    assert_eq!(calibrated.relay_tip, original.relay_tip);
    assert_eq!(calibrated.relay_tip_share_bps, original.relay_tip_share_bps);
    assert_eq!(calibrated.minimum_relay_tip, original.minimum_relay_tip);
    assert_eq!(calibrated.maximum_relay_tip, original.maximum_relay_tip);
    assert_eq!(calibrated.failure_fee, original.failure_fee);
}
