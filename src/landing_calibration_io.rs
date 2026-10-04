use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    landing::LandingProvider,
    landing_calibration::{
        LandingOutcome, LandingProbabilityCalibrator, ProbabilityPrior, ProviderOutcomeCounts,
    },
};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProviderInput {
    Direct,
    Jito,
    HeliusSender,
}

impl From<ProviderInput> for LandingProvider {
    fn from(value: ProviderInput) -> Self {
        match value {
            ProviderInput::Direct => LandingProvider::Direct,
            ProviderInput::Jito => LandingProvider::Jito,
            ProviderInput::HeliusSender => LandingProvider::HeliusSender,
        }
    }
}

fn default_weight() -> u64 {
    1
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct OutcomeRecord {
    provider: ProviderInput,
    success: bool,
    #[serde(default = "default_weight")]
    weight: u64,
}

#[derive(Debug, Error)]
pub enum CalibrationIoError {
    #[error("invalid calibration JSON on line {line}: {source}")]
    InvalidJson {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
}

pub fn parse_outcomes_jsonl(input: &str) -> Result<Vec<LandingOutcome>, CalibrationIoError> {
    let mut outcomes = Vec::new();

    for (index, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let record: OutcomeRecord =
            serde_json::from_str(line).map_err(|source| CalibrationIoError::InvalidJson {
                line: index + 1,
                source,
            })?;

        outcomes.push(LandingOutcome {
            provider: record.provider.into(),
            success: record.success,
            weight: record.weight,
        });
    }

    Ok(outcomes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ProviderCalibrationReport {
    pub successes: u64,
    pub failures: u64,
    pub samples: u64,
    pub success_probability_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LandingCalibrationReport {
    pub prior_successes: u64,
    pub prior_failures: u64,
    pub direct: ProviderCalibrationReport,
    pub jito: ProviderCalibrationReport,
    pub helius_sender: ProviderCalibrationReport,
}

pub fn build_calibration_report(
    calibrator: &LandingProbabilityCalibrator,
) -> LandingCalibrationReport {
    let prior = calibrator.prior();

    LandingCalibrationReport {
        prior_successes: prior.successes,
        prior_failures: prior.failures,
        direct: provider_report(calibrator, LandingProvider::Direct),
        jito: provider_report(calibrator, LandingProvider::Jito),
        helius_sender: provider_report(calibrator, LandingProvider::HeliusSender),
    }
}

pub fn calibrate_jsonl(
    input: &str,
    prior: ProbabilityPrior,
) -> Result<LandingCalibrationReport, CalibrationIoError> {
    let mut calibrator = LandingProbabilityCalibrator::new(prior)
        .expect("caller must provide a prior with nonzero total weight");
    calibrator.observe_many(parse_outcomes_jsonl(input)?);
    Ok(build_calibration_report(&calibrator))
}

fn provider_report(
    calibrator: &LandingProbabilityCalibrator,
    provider: LandingProvider,
) -> ProviderCalibrationReport {
    let ProviderOutcomeCounts {
        successes,
        failures,
    } = calibrator.counts(provider);

    ProviderCalibrationReport {
        successes,
        failures,
        samples: successes.saturating_add(failures),
        success_probability_bps: calibrator.estimate_success_probability_bps(provider),
    }
}
