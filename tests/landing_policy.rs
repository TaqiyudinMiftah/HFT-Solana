use hft_solana::{
    landing::{
        choose_landing_path, evaluate_landing_candidate, LandingCandidate, LandingPaperStats,
        LandingPolicyConfig, LandingProvider,
    },
    opportunity::Opportunity,
};

fn opportunity(edge: i128) -> Opportunity {
    Opportunity {
        cycle_id: 7,
        amount_in: 1_000_000,
        expected_out: 1_000_000,
        expected_effective_profit: edge,
        expected_cu: 150_000,
        created_ns: 1,
    }
}

fn config() -> LandingPolicyConfig {
    LandingPolicyConfig {
        base_fee: 5_000,
        minimum_net_if_landed: 10_000,
        minimum_expected_value: 1_000,
        max_tip_share_bps: 6_000,
    }
}

#[test]
fn rejects_tip_that_consumes_too_much_edge() {
    let result = evaluate_landing_candidate(
        &opportunity(100_000),
        LandingCandidate {
            provider: LandingProvider::Jito,
            success_probability_bps: 9_000,
            priority_fee: 5_000,
            relay_tip: 70_000,
            failure_fee: 10_000,
        },
        config(),
    );

    assert!(result.is_none());
}

#[test]
fn expected_value_accounts_for_failure_fee() {
    let choice = evaluate_landing_candidate(
        &opportunity(100_000),
        LandingCandidate {
            provider: LandingProvider::HeliusSender,
            success_probability_bps: 8_000,
            priority_fee: 5_000,
            relay_tip: 20_000,
            failure_fee: 12_000,
        },
        config(),
    )
    .unwrap();

    // landed net = 100_000 - 5_000 base - 5_000 priority - 20_000 tip = 70_000
    // EV = 0.8 * 70_000 - 0.2 * 12_000 = 53_600
    assert_eq!(choice.net_if_landed, 70_000);
    assert_eq!(choice.expected_value, 53_600);
}

#[test]
fn chooses_higher_ev_not_merely_highest_landing_probability() {
    let choice = choose_landing_path(
        &opportunity(100_000),
        [
            LandingCandidate {
                provider: LandingProvider::Jito,
                success_probability_bps: 9_500,
                priority_fee: 5_000,
                relay_tip: 50_000,
                failure_fee: 10_000,
            },
            LandingCandidate {
                provider: LandingProvider::HeliusSender,
                success_probability_bps: 8_500,
                priority_fee: 5_000,
                relay_tip: 15_000,
                failure_fee: 10_000,
            },
            LandingCandidate {
                provider: LandingProvider::Direct,
                success_probability_bps: 6_000,
                priority_fee: 10_000,
                relay_tip: 0,
                failure_fee: 15_000,
            },
        ],
        config(),
    )
    .unwrap();

    assert_eq!(choice.provider, LandingProvider::HeliusSender);
    assert!(choice.expected_value > 0);
}

#[test]
fn invalid_probability_tip_cap_and_low_net_are_rejected() {
    let invalid_probability = LandingCandidate {
        provider: LandingProvider::Direct,
        success_probability_bps: 10_001,
        priority_fee: 0,
        relay_tip: 0,
        failure_fee: 0,
    };
    assert!(
        evaluate_landing_candidate(&opportunity(100_000), invalid_probability, config()).is_none()
    );

    let invalid_tip_cap = LandingPolicyConfig {
        max_tip_share_bps: 10_001,
        ..config()
    };
    assert!(evaluate_landing_candidate(
        &opportunity(100_000),
        LandingCandidate {
            provider: LandingProvider::Direct,
            success_probability_bps: 10_000,
            priority_fee: 0,
            relay_tip: 0,
            failure_fee: 0,
        },
        invalid_tip_cap,
    )
    .is_none());

    let low_net = LandingCandidate {
        provider: LandingProvider::Jito,
        success_probability_bps: 10_000,
        priority_fee: 5_000,
        relay_tip: 10_000,
        failure_fee: 5_000,
    };
    assert!(evaluate_landing_candidate(&opportunity(20_000), low_net, config()).is_none());
}


#[test]
fn paper_stats_track_provider_mix_skips_and_ev() {
    let direct = evaluate_landing_candidate(
        &opportunity(100_000),
        LandingCandidate {
            provider: LandingProvider::Direct,
            success_probability_bps: 8_000,
            priority_fee: 5_000,
            relay_tip: 0,
            failure_fee: 10_000,
        },
        config(),
    )
    .unwrap();

    let jito = evaluate_landing_candidate(
        &opportunity(100_000),
        LandingCandidate {
            provider: LandingProvider::Jito,
            success_probability_bps: 9_000,
            priority_fee: 5_000,
            relay_tip: 20_000,
            failure_fee: 10_000,
        },
        config(),
    )
    .unwrap();

    let mut stats = LandingPaperStats::default();
    stats.record_choice(direct);
    stats.record_choice(jito);
    stats.record_skip();

    assert_eq!(stats.evaluated, 3);
    assert_eq!(stats.selected, 2);
    assert_eq!(stats.skipped, 1);
    assert_eq!(stats.direct, 1);
    assert_eq!(stats.jito, 1);
    assert_eq!(stats.helius_sender, 0);
    assert_eq!(
        stats.expected_value_total,
        direct.expected_value + jito.expected_value
    );
}
