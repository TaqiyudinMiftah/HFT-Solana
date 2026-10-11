//! Bounded, independent RPC snapshot observations; no gRPC or submission.

use std::{env, error::Error, fs, io, time::Duration};

use hft_solana::{
    bootstrap_rpc::{
        apply_bootstrap_catchup, fetch_bootstrap_snapshot, BootstrapRpcConfig, BootstrapSnapshot,
    },
    feed::FeedEvent,
    landing::choose_landing_path,
    paper_config::PaperConfig,
    search::{capture_active_cycle, QuoteContext},
};

struct Options {
    path: String,
    samples: u64,
    interval_secs: u64,
}

fn options(mut args: impl Iterator<Item = String>) -> io::Result<Options> {
    let mut result = Options {
        path: args.next().ok_or_else(|| {
            io::Error::other(
                "usage: paper_bootstrap <config.json> [--samples N] [--interval-secs N]",
            )
        })?,
        samples: 1,
        interval_secs: 5,
    };
    while let Some(flag) = args.next() {
        let target = match flag.as_str() {
            "--samples" => &mut result.samples,
            "--interval-secs" => &mut result.interval_secs,
            _ => return Err(io::Error::other("unknown paper_bootstrap option")),
        };
        *target = args
            .next()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                io::Error::other("sample count and interval must be positive integers")
            })?;
    }
    Ok(result)
}

// RPC does not expose bank identity/write versions. Accept one complete
// getMultipleAccounts context and never merge observations into stream state.
fn accept_snapshot(snapshot: &BootstrapSnapshot, last_slot: &mut Option<u64>) -> io::Result<bool> {
    if snapshot.events.is_empty()
        || snapshot.events.len() != snapshot.explicit_accounts
        || snapshot.scoped_dlmm_bin_arrays != 0
        || snapshot.events.iter().any(|event| {
            !matches!(event, FeedEvent::Account(update)
                if update.is_startup && update.slot == snapshot.max_context_slot)
        })
    {
        return Err(io::Error::other(
            "RPC observation requires one complete account context",
        ));
    }
    if last_slot.is_some_and(|previous| snapshot.max_context_slot <= previous) {
        return Ok(false);
    }
    *last_slot = Some(snapshot.max_context_slot);
    Ok(true)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let options = options(env::args().skip(1))?;
    let config = PaperConfig::from_json_str(&fs::read_to_string(options.path)?)?;
    let built = config.clone().build()?;
    if built.account_filters.len() > 100 || !built.dlmm_bin_pairs.is_empty() {
        return Err(io::Error::other(
            "RPC observations support at most 100 explicit accounts and no DLMM discovery; use Yellowstone bootstrap for multi-request configurations",
        ).into());
    }
    if built.landing.is_some()
        && config.cycles.iter().any(|cycle| {
            config.tokens[cycle.start_token as usize]
                != "So11111111111111111111111111111111111111112"
        })
    {
        return Err(io::Error::other(
            "landing costs are lamports: use WSOL start assets or remove landing from the RPC observation config",
        ).into());
    }
    let timeout_secs = env::var("HFT_BOOTSTRAP_TIMEOUT_SECS")
        .unwrap_or_else(|_| "30".to_owned())
        .parse::<u64>()?;
    if timeout_secs == 0 {
        return Err(io::Error::other("HFT_BOOTSTRAP_TIMEOUT_SECS must be positive").into());
    }
    let rpc_config = BootstrapRpcConfig {
        rpc_url: env::var("HFT_SOLANA_RPC_URL")?,
        explicit_accounts: built.account_filters,
        dlmm_bin_pairs: Vec::new(),
    };
    let mut last_slot = None;
    for sample in 1..=options.samples {
        let snapshot = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            fetch_bootstrap_snapshot(&rpc_config),
        )
        .await
        .map_err(|_| io::Error::other("RPC bootstrap timed out"))?
        // Provider errors may include the credential-bearing URL.
        .map_err(|_| io::Error::other("RPC bootstrap failed; provider details omitted"))?;
        if accept_snapshot(&snapshot, &mut last_slot)? {
            // Fresh pipeline: no invented bank identity, cross-sample state, or
            // assumptions about processed fork ancestry or same-slot ordering.
            let mut built = config.clone().build()?;
            let created_ns = u64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos(),
            )?;
            let result = apply_bootstrap_catchup(
                &mut built.pipeline,
                snapshot,
                std::iter::empty(),
                created_ns,
            );
            println!(
                "PAPER_BOOTSTRAP context_slot={} explicit_accounts={} initial_opportunities={} sample={sample}",
                result.context_slot, result.explicit_accounts, result.initial.opportunities.len()
            );
            for index in 0..config.pools.len() {
                let pool_id = u32::try_from(index)?;
                if !built.pipeline.store().is_ready(pool_id) {
                    return Err(
                        io::Error::other(format!("pool {pool_id} is not quote-ready")).into(),
                    );
                }
                println!("PAPER_POOL_READY pool={pool_id}");
            }
            for (cycle, settings) in built
                .pipeline
                .engine()
                .graph()
                .cycles
                .iter()
                .zip(&config.cycles)
            {
                let snapshot = capture_active_cycle(
                    cycle,
                    built.pipeline.store(),
                    settings.search.max_slot_skew,
                )?;
                let quote = snapshot.quote_at(
                    cycle,
                    settings.search.minimum_probe,
                    QuoteContext {
                        current_timestamp: created_ns / 1_000_000_000,
                        current_slot: snapshot.max_slot(),
                    },
                )?;
                snapshot.validate(built.pipeline.store())?;
                println!(
                    "PAPER_QUOTE cycle={} amount_in={} amount_out={} effective_profit={}",
                    cycle.id, quote.amount_in, quote.amount_out, quote.effective_profit
                );
            }
            for opportunity in result.initial.opportunities {
                println!(
                    "PAPER_RPC_OPPORTUNITY sample={sample} context_slot={} cycle={} amount_in={} expected_out={} effective_profit={}",
                    result.context_slot, opportunity.cycle_id, opportunity.amount_in,
                    opportunity.expected_out, opportunity.expected_effective_profit
                );
                if let Some(policy) = &built.landing {
                    match choose_landing_path(&opportunity, policy.candidates.iter().copied(), policy.config) {
                        Some(choice) => println!(
                            "PAPER_RPC_LANDING sample={sample} cycle={} provider={:?} net_if_landed={} expected_value={}",
                            opportunity.cycle_id, choice.provider, choice.net_if_landed, choice.expected_value
                        ),
                        None => println!("PAPER_RPC_LANDING_SKIP sample={sample} cycle={}", opportunity.cycle_id),
                    }
                }
            }
        } else {
            println!(
                "PAPER_RPC_SKIP sample={sample} context_slot={} reason=non_increasing_context",
                snapshot.max_context_slot
            );
        }
        if sample < options.samples {
            // Delay after completing the request; never catch up missed ticks
            // with a burst of provider requests.
            tokio::time::sleep(Duration::from_secs(options.interval_secs)).await;
        }
    }
    println!("Paper snapshot check finished; no transactions submitted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hft_solana::feed::AccountUpdate;

    fn snapshot(slot: u64, account_slots: &[u64]) -> BootstrapSnapshot {
        BootstrapSnapshot {
            max_context_slot: slot,
            explicit_accounts: account_slots.len(),
            events: account_slots
                .iter()
                .enumerate()
                .map(|(index, slot)| {
                    FeedEvent::Account(AccountUpdate {
                        pubkey: [index as u8; 32],
                        owner: [9; 32],
                        slot: *slot,
                        write_version: 0,
                        generation: 0,
                        bank_id: None,
                        is_startup: true,
                        data: Vec::new(),
                    })
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn refuses_partial_or_mixed_contexts_before_quoting() {
        let mut last = None;
        assert!(accept_snapshot(&snapshot(100, &[99, 100]), &mut last).is_err());
        let mut partial = snapshot(100, &[100]);
        partial.explicit_accounts = 2;
        assert!(accept_snapshot(&partial, &mut last).is_err());
        assert_eq!(last, None);
    }

    #[test]
    fn suppresses_same_slot_and_older_observations() {
        let mut last = None;
        assert!(accept_snapshot(&snapshot(100, &[100, 100]), &mut last).unwrap());
        assert!(!accept_snapshot(&snapshot(100, &[100, 100]), &mut last).unwrap());
        assert!(!accept_snapshot(&snapshot(99, &[99, 99]), &mut last).unwrap());
        assert_eq!(last, Some(100));
        assert!(accept_snapshot(&snapshot(101, &[101, 101]), &mut last).unwrap());
    }

    #[test]
    fn refuses_live_events_without_rpc_startup_semantics() {
        let mut value = snapshot(100, &[100]);
        if let FeedEvent::Account(update) = &mut value.events[0] {
            update.is_startup = false;
        }
        assert!(accept_snapshot(&value, &mut None).is_err());
    }

    #[test]
    fn refuses_unbounded_or_burst_cli_values() {
        for args in [
            vec!["config.json", "--samples", "0"],
            vec!["config.json", "--interval-secs", "0"],
            vec!["config.json", "--samples"],
        ] {
            assert!(options(args.into_iter().map(str::to_owned)).is_err());
        }
    }
}
