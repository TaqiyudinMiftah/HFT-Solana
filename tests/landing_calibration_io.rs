#![cfg(feature = "calibration")]

use hft_solana::{
    landing_calibration::ProbabilityPrior,
    landing_calibration_io::{calibrate_jsonl, parse_outcomes_jsonl, CalibrationIoError},
};

#[test]
fn parses_jsonl_with_default_and_weighted_records() {
    let input = r#"
{"provider":"jito","success":true}
{"provider":"jito","success":false,"weight":2}

{"provider":"helius_sender","success":true,"weight":3}
"#;

    let outcomes = parse_outcomes_jsonl(input).unwrap();
    assert_eq!(outcomes.len(), 3);
    assert_eq!(outcomes[0].weight, 1);
    assert_eq!(outcomes[1].weight, 2);
    assert_eq!(outcomes[2].weight, 3);
}

#[test]
fn invalid_json_reports_source_line() {
    let input = r#"
{"provider":"jito","success":true}
not-json
"#;

    let error = parse_outcomes_jsonl(input).unwrap_err();
    assert!(matches!(
        error,
        CalibrationIoError::InvalidJson { line: 3, .. }
    ));
}

#[test]
fn report_contains_counts_and_smoothed_probabilities() {
    let input = r#"
{"provider":"jito","success":true,"weight":3}
{"provider":"jito","success":false}
{"provider":"direct","success":false}
"#;

    let report = calibrate_jsonl(
        input,
        ProbabilityPrior {
            successes: 1,
            failures: 1,
        },
    )
    .unwrap();

    assert_eq!(report.prior_successes, 1);
    assert_eq!(report.prior_failures, 1);

    assert_eq!(report.jito.successes, 3);
    assert_eq!(report.jito.failures, 1);
    assert_eq!(report.jito.samples, 4);
    assert_eq!(report.jito.success_probability_bps, 6_667);

    assert_eq!(report.direct.successes, 0);
    assert_eq!(report.direct.failures, 1);
    assert_eq!(report.direct.success_probability_bps, 3_333);

    assert_eq!(report.helius_sender.samples, 0);
    assert_eq!(report.helius_sender.success_probability_bps, 5_000);
}
