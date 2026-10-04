use std::{env, fs, io};

use hft_solana::{
    landing_calibration::{LandingProbabilityCalibrator, ProbabilityPrior},
    landing_calibration_io::{build_calibration_report, parse_outcomes_jsonl},
};

fn parse_prior_arg(
    value: Option<String>,
    default: u64,
    name: &'static str,
) -> Result<u64, io::Error> {
    match value {
        Some(value) => value.parse::<u64>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name} must be an unsigned integer"),
            )
        }),
        None => Ok(default),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: calibrate_landing <outcomes.jsonl> [prior_successes] [prior_failures]",
        )
    })?;

    let prior = ProbabilityPrior {
        successes: parse_prior_arg(args.next(), 1, "prior_successes")?,
        failures: parse_prior_arg(args.next(), 1, "prior_failures")?,
    };

    let mut calibrator = LandingProbabilityCalibrator::new(prior).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "prior_successes + prior_failures must be greater than zero",
        )
    })?;

    if args.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "too many arguments",
        )
        .into());
    }

    let input = fs::read_to_string(path)?;
    calibrator.observe_many(parse_outcomes_jsonl(&input)?);

    let report = build_calibration_report(&calibrator);
    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}
