//! Bounded, independent RPC snapshot observations; no gRPC or submission.

use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs, io,
    time::{Duration, Instant},
};

use hft_solana::{
    bootstrap_rpc::{
        apply_bootstrap_catchup, fetch_bootstrap_snapshot, BootstrapRpcConfig, BootstrapSnapshot,
    },
    feed::FeedEvent,
    landing::choose_landing_path,
    opportunity_engine::SearchStats,
    paper_config::PaperConfig,
    search::{capture_active_cycle, QuoteContext},
};

struct Options {
    path: String,
    samples: u64,
    interval_secs: u64,
    diagnostic_sizes: Vec<u64>,
    report_path: Option<String>,
}

fn options(mut args: impl Iterator<Item = String>) -> io::Result<Options> {
    let mut result = Options {
        path: args.next().ok_or_else(|| {
            io::Error::other(
                "usage: paper_bootstrap <config.json> [--samples N] [--interval-secs N] [--diagnostic-sizes raw,raw,...] [--report new-report.json]",
            )
        })?,
        samples: 1,
        interval_secs: 5,
        diagnostic_sizes: Vec::new(),
        report_path: None,
    };
    while let Some(flag) = args.next() {
        if flag == "--diagnostic-sizes" {
            result.diagnostic_sizes = args
                .next()
                .ok_or_else(|| io::Error::other("missing diagnostic sizes"))?
                .split(',')
                .map(|raw| {
                    raw.parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or_else(|| {
                            io::Error::other("diagnostic sizes must be positive raw integers")
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if result.diagnostic_sizes.len() > 16 {
                return Err(io::Error::other("at most 16 diagnostic sizes"));
            }
            result.diagnostic_sizes.sort_unstable();
            result.diagnostic_sizes.dedup();
            continue;
        }
        if flag == "--report" {
            result.report_path = Some(
                args.next()
                    .ok_or_else(|| io::Error::other("missing report path"))?,
            );
            continue;
        }
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

fn search_stats(stats: SearchStats) -> serde_json::Value {
    serde_json::json!({
        "evaluated":stats.evaluated, "snapshot_unavailable":stats.snapshot_unavailable,
        "duplicate_snapshot":stats.duplicate_snapshot, "probe_quote_failed":stats.probe_quote_failed,
        "nonpositive_probe":stats.nonpositive_probe, "sizing_failed":stats.sizing_failed,
        "below_minimum_profit":stats.below_minimum_profit, "stale_snapshot":stats.stale_snapshot,
        "opportunities":stats.opportunities,
    })
}

#[derive(serde::Serialize)]
struct CurveStats {
    cycle: u32,
    amount_in: u64,
    observations: u64,
    minimum_profit: i128,
    maximum_profit: i128,
    best_context_slot: u64,
}

fn record_curve(
    curves: &mut BTreeMap<(u32, u64), CurveStats>,
    cycle: u32,
    amount: u64,
    profit: i128,
    slot: u64,
) {
    let entry = curves.entry((cycle, amount)).or_insert(CurveStats {
        cycle,
        amount_in: amount,
        observations: 0,
        minimum_profit: profit,
        maximum_profit: profit,
        best_context_slot: slot,
    });
    entry.observations = entry.observations.saturating_add(1);
    entry.minimum_profit = entry.minimum_profit.min(profit);
    if profit > entry.maximum_profit {
        entry.maximum_profit = profit;
        entry.best_context_slot = slot;
    }
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
    for cycle in &config.cycles {
        if options
            .diagnostic_sizes
            .iter()
            .any(|amount| *amount < cycle.search.minimum_probe || *amount > cycle.search.max_size)
        {
            return Err(io::Error::other(
                "diagnostic sizes must be within every cycle's configured search bounds",
            )
            .into());
        }
    }
    if options
        .report_path
        .as_ref()
        .is_some_and(|path| std::path::Path::new(path).exists())
    {
        return Err(io::Error::other("report exists; choose a new path").into());
    }
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
    let started = Instant::now();
    let mut accepted_samples = 0u64;
    let mut skipped_samples = 0u64;
    let mut quote_failures = 0u64;
    let mut diagnostic_disagreements = 0u64;
    let mut landing_selected = 0u64;
    let mut landing_rejected = 0u64;
    let mut unchanged_samples = 0u64;
    let mut previous_accounts = BTreeMap::<[u8; 32], ([u8; 32], Vec<u8>)>::new();
    let mut total_search = BTreeMap::<String, u64>::new();
    let mut curves = BTreeMap::new();
    let mut total_rpc_us = 0u128;
    let mut total_search_us = 0u128;
    let mut total_diagnostic_us = 0u128;
    for sample in 1..=options.samples {
        let rpc_started = Instant::now();
        let snapshot = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            fetch_bootstrap_snapshot(&rpc_config),
        )
        .await
        .map_err(|_| io::Error::other("RPC bootstrap timed out"))?
        // Provider errors may include the credential-bearing URL.
        .map_err(|_| io::Error::other("RPC bootstrap failed; provider details omitted"))?;
        let rpc_us = rpc_started.elapsed().as_micros();
        total_rpc_us += rpc_us;
        if accept_snapshot(&snapshot, &mut last_slot)? {
            accepted_samples += 1;
            let baseline = previous_accounts.is_empty();
            let mut changed_accounts = 0usize;
            for event in &snapshot.events {
                if let FeedEvent::Account(update) = event {
                    let current = (update.owner, update.data.clone());
                    if previous_accounts
                        .get(&update.pubkey)
                        .is_some_and(|previous| previous != &current)
                    {
                        changed_accounts += 1;
                    }
                    previous_accounts.insert(update.pubkey, current);
                }
            }
            if !baseline && changed_accounts == 0 {
                unchanged_samples += 1;
            }
            // Fresh pipeline: no invented bank identity, cross-sample state, or
            // assumptions about processed fork ancestry or same-slot ordering.
            let mut built = config.clone().build()?;
            let created_ns = u64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos(),
            )?;
            let search_started = Instant::now();
            let result = apply_bootstrap_catchup(
                &mut built.pipeline,
                snapshot,
                std::iter::empty(),
                created_ns,
            );
            let search_us = search_started.elapsed().as_micros();
            total_search_us += search_us;
            println!(
                "PAPER_BOOTSTRAP context_slot={} explicit_accounts={} initial_opportunities={} sample={sample}",
                result.context_slot, result.explicit_accounts, result.initial.opportunities.len()
            );
            let stats = search_stats(built.pipeline.engine().stats());
            for (name, value) in stats.as_object().unwrap() {
                let total = total_search.entry(name.clone()).or_default();
                *total = total.saturating_add(value.as_u64().unwrap());
            }
            println!("PAPER_SEARCH_STATS sample={sample} stats={stats}");
            for index in 0..config.pools.len() {
                let pool_id = u32::try_from(index)?;
                if !built.pipeline.store().is_ready(pool_id) {
                    return Err(
                        io::Error::other(format!("pool {pool_id} is not quote-ready")).into(),
                    );
                }
                println!("PAPER_POOL_READY pool={pool_id}");
            }
            let diagnostic_started = Instant::now();
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
                let context = QuoteContext {
                    current_timestamp: created_ns / 1_000_000_000,
                    current_slot: snapshot.max_slot(),
                };
                let probe = snapshot.quote_at(cycle, settings.search.minimum_probe, context);
                match &probe {
                    Ok(quote) => {
                        record_curve(
                            &mut curves,
                            cycle.id,
                            quote.amount_in,
                            quote.effective_profit,
                            result.context_slot,
                        );
                        println!(
                            "PAPER_QUOTE cycle={} amount_in={} amount_out={} effective_profit={}",
                            cycle.id, quote.amount_in, quote.amount_out, quote.effective_profit
                        );
                    }
                    Err(_) => {
                        quote_failures += 1;
                        println!(
                            "PAPER_QUOTE_FAILED sample={sample} cycle={} amount_in={}",
                            cycle.id, settings.search.minimum_probe
                        );
                    }
                }
                let mut positive_larger = false;
                for &amount in &options.diagnostic_sizes {
                    if amount == settings.search.minimum_probe {
                        continue;
                    }
                    match snapshot.quote_at(cycle, amount, context) {
                        Ok(quote) => {
                            positive_larger |= quote.effective_profit > 0;
                            record_curve(
                                &mut curves,
                                cycle.id,
                                amount,
                                quote.effective_profit,
                                result.context_slot,
                            );
                            println!("PAPER_DIAGNOSTIC sample={sample} cycle={} amount_in={} amount_out={} effective_profit={}",cycle.id,amount,quote.amount_out,quote.effective_profit);
                        }
                        Err(_) => {
                            quote_failures += 1;
                            println!("PAPER_DIAGNOSTIC_FAILED sample={sample} cycle={} amount_in={amount}",cycle.id);
                        }
                    }
                }
                if positive_larger
                    && probe
                        .as_ref()
                        .is_ok_and(|quote| quote.effective_profit <= 0)
                {
                    diagnostic_disagreements += 1;
                    println!("PAPER_PROBE_DISAGREEMENT sample={sample} cycle={} reason=nonpositive_probe_positive_larger_grid_quote",cycle.id);
                }
                snapshot.validate(built.pipeline.store())?;
            }
            let diagnostic_us = diagnostic_started.elapsed().as_micros();
            total_diagnostic_us += diagnostic_us;
            println!("PAPER_RPC_METRICS sample={sample} rpc_us={rpc_us} bootstrap_search_us={search_us} diagnostic_us={diagnostic_us} changed_accounts={changed_accounts} baseline={baseline}");
            for opportunity in result.initial.opportunities {
                println!(
                    "PAPER_RPC_OPPORTUNITY sample={sample} context_slot={} cycle={} amount_in={} expected_out={} effective_profit={}",
                    result.context_slot, opportunity.cycle_id, opportunity.amount_in,
                    opportunity.expected_out, opportunity.expected_effective_profit
                );
                if let Some(policy) = &built.landing {
                    match choose_landing_path(
                        &opportunity,
                        policy.candidates.iter().copied(),
                        policy.config,
                    ) {
                        Some(choice) => {
                            landing_selected += 1;
                            println!(
                            "PAPER_RPC_LANDING sample={sample} cycle={} provider={:?} net_if_landed={} expected_value={}",
                            opportunity.cycle_id, choice.provider, choice.net_if_landed, choice.expected_value
                        );
                        }
                        None => {
                            landing_rejected += 1;
                            println!(
                                "PAPER_RPC_LANDING_SKIP sample={sample} cycle={}",
                                opportunity.cycle_id
                            );
                        }
                    }
                }
            }
        } else {
            skipped_samples += 1;
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
    let elapsed = started.elapsed().as_secs_f64();
    let report = serde_json::json!({"mode":"independent_rpc_snapshots", "transactions_submitted":0,
        "requested_samples":options.samples, "accepted_samples":accepted_samples, "skipped_samples":skipped_samples,
        "elapsed_seconds":elapsed, "interval_seconds":options.interval_secs,
        "pools":config.pools.len(), "cycles":config.cycles.len(), "last_context_slot":last_slot,
        "search":total_search,"landing_selected":landing_selected,"landing_rejected":landing_rejected,
        "quote_failures":quote_failures,"probe_grid_disagreements":diagnostic_disagreements,
        "unchanged_account_samples":unchanged_samples,"rpc_total_us":total_rpc_us,
        "bootstrap_search_total_us":total_search_us,"diagnostic_total_us":total_diagnostic_us,
        "curves":curves.values().collect::<Vec<_>>()});
    println!("PAPER_RUN_SUMMARY {report}");
    if let Some(path) = options.report_path {
        use std::io::Write;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        writeln!(output, "{}", serde_json::to_string_pretty(&report)?)?;
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
            vec!["config.json", "--diagnostic-sizes", "1,0,100"],
            vec!["config.json", "--diagnostic-sizes", "1,-1,100"],
            vec!["config.json", "--report"],
        ] {
            assert!(options(args.into_iter().map(str::to_owned)).is_err());
        }
    }

    #[test]
    fn curve_summary_retains_profit_extremes_and_best_context() {
        let mut curves = BTreeMap::new();
        record_curve(&mut curves, 0, 1000, -10, 100);
        record_curve(&mut curves, 0, 1000, 50, 101);
        record_curve(&mut curves, 0, 1000, 20, 102);
        record_curve(&mut curves, 0, 10000, 5, 102);
        let entry = &curves[&(0, 1000)];
        assert_eq!(entry.observations, 3);
        assert_eq!(entry.minimum_profit, -10);
        assert_eq!(entry.maximum_profit, 50);
        assert_eq!(entry.best_context_slot, 101);
        assert_eq!(curves.len(), 2);
    }
}
