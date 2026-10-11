//! Validate public index candidates using RPC and the existing state decoders.
use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs, io,
    path::Path,
    str::FromStr,
    time::{Duration, Instant},
};

#[path = "support/discovery_activity.rs"]
mod activity;
use activity::{ActivityStats, ActivityTracker};

use hft_solana::{
    bootstrap_rpc::{
        apply_bootstrap_catchup, fetch_bootstrap_snapshot, BootstrapRpcConfig, BootstrapSnapshot,
    },
    decode::{meteora_damm, mint::inspect_mint, pump, raydium},
    feed::{AccountUpdate, FeedEvent},
    paper_config::PaperConfig,
};
use serde::Deserialize;
use serde_json::{json, Value};
use solana_pubkey::Pubkey;

const WSOL: &str = "So11111111111111111111111111111111111111112";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const CPMM: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
const DAMM: &str = "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG";
const PUMP_AMM: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
const PUMP: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const PUMP_FEE: &str = "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ";

#[derive(Clone, Deserialize)]
struct Candidate {
    address: String,
    kind: String,
    mints: [String; 2],
    tvl: f64,
    #[serde(rename = "volume24h")]
    volume_24h: f64,
}

#[derive(Deserialize)]
struct Candidates {
    candidates: Vec<Candidate>,
}

#[derive(Clone)]
struct DerivedPool {
    candidate: Candidate,
    config: Value,
    mints: [String; 2],
    activity: ActivityStats,
}

struct Options {
    candidates: String,
    template: String,
    output: String,
    maximum: usize,
    activity_samples: usize,
    activity_interval_secs: u64,
    activity_report: Option<String>,
}

fn options(mut args: impl Iterator<Item = String>) -> io::Result<Options> {
    let usage = || {
        io::Error::other("usage: paper_discover <candidates.json> <template.json> <new-config.json> [max-pools=15] [--activity-samples 3..61] [--activity-interval-secs 1..30] [--activity-report new-report.json]")
    };
    let mut result = Options {
        candidates: args.next().ok_or_else(usage)?,
        template: args.next().ok_or_else(usage)?,
        output: args.next().ok_or_else(usage)?,
        maximum: 15,
        activity_samples: 0,
        activity_interval_secs: 5,
        activity_report: None,
    };
    let mut remaining = args.peekable();
    if remaining.peek().is_some_and(|arg| !arg.starts_with("--")) {
        result.maximum = remaining.next().unwrap().parse().map_err(|_| usage())?;
    }
    while let Some(flag) = remaining.next() {
        let value = remaining.next().ok_or_else(usage)?;
        match flag.as_str() {
            "--activity-samples" => result.activity_samples = value.parse().map_err(|_| usage())?,
            "--activity-interval-secs" => {
                result.activity_interval_secs = value.parse().map_err(|_| usage())?
            }
            "--activity-report" => result.activity_report = Some(value),
            _ => return Err(usage()),
        }
    }
    if !(2..=20).contains(&result.maximum)
        || !(1..=30).contains(&result.activity_interval_secs)
        || (result.activity_samples != 0 && !(3..=61).contains(&result.activity_samples))
        || (result.activity_report.is_some() && result.activity_samples == 0)
    {
        return Err(usage());
    }
    for path in std::iter::once(&result.output).chain(result.activity_report.iter()) {
        if Path::new(path).exists() {
            return Err(io::Error::other(
                "output exists; choose new config/report paths",
            ));
        }
    }
    if result.activity_report.as_ref() == Some(&result.output) {
        return Err(io::Error::other("config and report paths must differ"));
    }
    Ok(result)
}

fn key(bytes: [u8; 32]) -> String {
    Pubkey::new_from_array(bytes).to_string()
}

fn derive(
    candidate: Candidate,
    account: &AccountUpdate,
    now: u64,
) -> Result<DerivedPool, Box<dyn Error>> {
    if account.pubkey != Pubkey::from_str(&candidate.address)?.to_bytes() {
        return Err(io::Error::other("candidate account identity mismatch").into());
    }
    let (config, mints) = match candidate.kind.as_str() {
        "raydium_cpmm" if key(account.owner) == CPMM => {
            let pool = raydium::decode_pool_state(&account.data)?;
            // Packed official PoolState: status bit 2 disables swaps;
            // open_time is at 373. These gates are discovery restrictions only.
            if account.data[329] & 4 != 0
                || u64::from_le_bytes(account.data[373..381].try_into()?) > now
            {
                return Err(io::Error::other("Raydium pool is not open for swaps").into());
            }
            let mints = [key(pool.token_0_mint), key(pool.token_1_mint)];
            (
                json!({"kind": "raydium_cpmm", "pool": candidate.address,
                "amm_config": key(pool.amm_config), "vault_0": key(pool.token_0_vault),
                "vault_1": key(pool.token_1_vault), "mint_0": mints[0], "mint_1": mints[1]}),
                mints,
            )
        }
        "meteora_damm" if key(account.owner) == DAMM => {
            let pool = meteora_damm::decode_pool(&account.data)?;
            if pool.pool_status != 0
                || pool.activation_type > 1
                || pool.activation_point
                    > if pool.activation_type == 0 {
                        account.slot
                    } else {
                        now
                    }
            {
                return Err(io::Error::other("Meteora pool is not active").into());
            }
            let mints = [key(pool.token_a_mint), key(pool.token_b_mint)];
            (
                json!({"kind": "meteora_damm", "pool": candidate.address,
                "vault_a": key(pool.token_a_vault), "vault_b": key(pool.token_b_vault),
                "mint_a": mints[0], "mint_b": mints[1]}),
                mints,
            )
        }
        "pump" if key(account.owner) == PUMP_AMM => {
            let pool = pump::decode_pool(&account.data)?;
            let amm = Pubkey::from_str(PUMP_AMM)?;
            let pump_program = Pubkey::from_str(PUMP)?;
            let fee_program = Pubkey::from_str(PUMP_FEE)?;
            // Official Pump AMM IDL seeds; all derived accounts are fetched
            // and decoded by the existing pipeline before config is saved.
            let fee_config =
                Pubkey::find_program_address(&[b"fee_config", amm.as_ref()], &fee_program).0;
            let global = Pubkey::find_program_address(&[b"global"], &pump_program).0;
            let amm_global = Pubkey::find_program_address(&[b"global_config"], &amm).0;
            let mints = [key(pool.base_mint), key(pool.quote_mint)];
            (
                json!({"kind":"pump", "pool":candidate.address, "fee_config":fee_config.to_string(),
                "pump_global":global.to_string(), "pump_amm_global_config":amm_global.to_string(),
                "base_vault":key(pool.pool_base_token_account), "quote_vault":key(pool.pool_quote_token_account),
                "base_mint":mints[0], "quote_mint":mints[1]}),
                mints,
            )
        }
        _ => return Err(io::Error::other("unsupported candidate kind or owner").into()),
    };
    let mut expected = candidate.mints.clone();
    let mut actual = mints.clone();
    expected.sort();
    actual.sort();
    if expected != actual || mints[0] == mints[1] || !mints.iter().any(|mint| mint == WSOL) {
        return Err(io::Error::other("on-chain mints do not match the index WSOL pair").into());
    }
    Ok(DerivedPool {
        candidate,
        config,
        mints,
        activity: ActivityStats::default(),
    })
}

fn compare_pool(a: &DerivedPool, b: &DerivedPool) -> std::cmp::Ordering {
    b.activity
        .compare_rate(&a.activity)
        .then_with(|| b.candidate.tvl.total_cmp(&a.candidate.tvl))
        .then_with(|| b.candidate.volume_24h.total_cmp(&a.candidate.volume_24h))
        .then_with(|| a.candidate.address.cmp(&b.candidate.address))
}

fn select(pools: &[DerivedPool], maximum: usize) -> Vec<DerivedPool> {
    let mut groups = BTreeMap::<[String; 2], Vec<DerivedPool>>::new();
    for pool in pools {
        let mut pair = pool.mints.clone();
        pair.sort();
        groups.entry(pair).or_default().push(pool.clone());
    }
    let mut groups: Vec<_> = groups
        .into_values()
        .filter(|group| {
            group.len() >= 2
                && (!group
                    .iter()
                    .any(|p| p.activity.first_context_slot.is_some())
                    || group.iter().any(|p| p.activity.changed_intervals > 0))
        })
        .collect();
    for group in &mut groups {
        // Without activity sampling retain the original index-volume order.
        group.sort_by(|a, b| {
            if a.activity.first_context_slot.is_some() || b.activity.first_context_slot.is_some() {
                compare_pool(a, b)
            } else {
                b.candidate
                    .volume_24h
                    .total_cmp(&a.candidate.volume_24h)
                    .then_with(|| b.candidate.tvl.total_cmp(&a.candidate.tvl))
                    .then_with(|| a.candidate.address.cmp(&b.candidate.address))
            }
        });
    }
    groups.sort_by(|a, b| {
        let score =
            |group: &[DerivedPool]| group.iter().map(|p| p.candidate.volume_24h).sum::<f64>();
        // Favor two observed active pools, then the stronger side of the pair.
        b[1].activity
            .compare_rate(&a[1].activity)
            .then_with(|| b[0].activity.compare_rate(&a[0].activity))
            .then_with(|| {
                score(b)
                    .total_cmp(&score(a))
                    .then_with(|| a[0].candidate.address.cmp(&b[0].candidate.address))
            })
    });
    let mut result = Vec::new();
    for group in groups {
        let take = group.len().min(4).min(maximum.saturating_sub(result.len()));
        if take >= 2 {
            result.extend(group.into_iter().take(take));
        }
    }
    result
}

fn watched_accounts(pool: &DerivedPool) -> io::Result<[String; 3]> {
    let fields = match pool.candidate.kind.as_str() {
        "raydium_cpmm" => ["vault_0", "vault_1"],
        "meteora_damm" => ["vault_a", "vault_b"],
        "pump" => ["base_vault", "quote_vault"],
        _ => return Err(io::Error::other("unsupported activity pool")),
    };
    let vault = |field| {
        pool.config[field]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| io::Error::other("missing activity vault address"))
    };
    Ok([
        pool.candidate.address.clone(),
        vault(fields[0])?,
        vault(fields[1])?,
    ])
}

async fn measure_activity(
    pools: &mut [DerivedPool],
    options: &Options,
) -> Result<Value, Box<dyn Error>> {
    let started = Instant::now();
    let watches = pools
        .iter()
        .map(watched_accounts)
        .collect::<Result<Vec<_>, _>>()?;
    let mut trackers: Vec<_> = pools.iter().map(|_| ActivityTracker::default()).collect();
    let mut requests = 0;
    // At most 33 complete pool+two-vault triples per request. Contexts from
    // different batches are independent ranking observations, never quote input.
    for sample in 0..options.activity_samples {
        for (batch, watches) in watches.chunks(33).enumerate() {
            let mut addresses: Vec<_> = watches.iter().flatten().cloned().collect();
            addresses.sort();
            addresses.dedup();
            let snapshot = fetch(addresses).await?;
            requests += 1;
            let accounts: BTreeMap<_, _> = snapshot
                .events
                .into_iter()
                .filter_map(|event| match event {
                    FeedEvent::Account(update) => Some((key(update.pubkey), update)),
                    _ => None,
                })
                .collect();
            let mut changed = 0;
            for (index, watch) in watches.iter().enumerate() {
                let values = watch
                    .iter()
                    .map(|address| {
                        accounts
                            .get(address)
                            .ok_or_else(|| io::Error::other("missing activity account"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let tracker = &mut trackers[batch * 33 + index];
                let before = tracker.stats.changed_intervals;
                tracker.observe(snapshot.max_context_slot, &values)?;
                changed += usize::from(tracker.stats.changed_intervals > before);
            }
            println!("PAPER_ACTIVITY_SAMPLE sample={} batch={} pools={} context_slot={} changed_pools={changed}", sample + 1, batch + 1, watches.len(), snapshot.max_context_slot);
            if batch + 1 < pools.len().div_ceil(33) {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
        if sample + 1 < options.activity_samples {
            tokio::time::sleep(Duration::from_secs(options.activity_interval_secs)).await;
        }
    }
    for (pool, tracker) in pools.iter_mut().zip(trackers) {
        pool.activity = tracker.stats;
    }
    let rows: Vec<_> = pools
        .iter()
        .map(|pool| {
            json!({
                "address":pool.candidate.address, "kind":pool.candidate.kind, "mints":pool.mints,
                "index_tvl":pool.candidate.tvl, "index_volume24h":pool.candidate.volume_24h,
                "activity":pool.activity,
            })
        })
        .collect();
    Ok(json!({
        "mode":"rpc_payload_change_ranking", "samples":options.activity_samples,
        "interval_seconds":options.activity_interval_secs, "rpc_requests":requests,
        "elapsed_seconds":started.elapsed().as_secs_f64(), "pools":rows,
        "note":"Payload changes are activity hints, not trade counts or profitable quotes. Batches are not a coherent quote snapshot.",
    }))
}

fn make_config(template: &Value, pools: &[DerivedPool]) -> Result<Value, Box<dyn Error>> {
    if pools.len() < 2 {
        return Err(io::Error::other("not enough validated, connected pools").into());
    }
    let search = template["cycles"][0]["search"].clone();
    if !search.is_object() {
        return Err(io::Error::other("template needs a cycle search config").into());
    }
    let mut tokens = vec![WSOL.to_owned()];
    for pool in pools {
        for mint in &pool.mints {
            if !tokens.contains(mint) {
                tokens.push(mint.clone());
            }
        }
    }
    let mut cycles = Vec::new();
    for (i, first) in pools.iter().enumerate() {
        for (j, second) in pools.iter().enumerate() {
            if i == j {
                continue;
            }
            let out = if first.mints[0] == WSOL {
                &first.mints[1]
            } else {
                &first.mints[0]
            };
            if !second.mints.contains(out) {
                continue;
            }
            let token = tokens.iter().position(|mint| mint == out).unwrap();
            cycles.push(json!({"start_token": 0, "edges": [
                {"pool": i, "direction": if first.mints[0] == WSOL {"a_to_b"} else {"b_to_a"}, "from_token": 0, "to_token": token},
                {"pool": j, "direction": if second.mints[0] == *out {"a_to_b"} else {"b_to_a"}, "from_token": token, "to_token": 0}],
                "search": search}));
        }
    }
    if cycles.is_empty() {
        return Err(io::Error::other("no mint-contiguous cycles").into());
    }
    let mut config = template.clone();
    config["tokens"] = json!(tokens);
    config["pools"] = json!(pools
        .iter()
        .map(|pool| pool.config.clone())
        .collect::<Vec<_>>());
    config["cycles"] = json!(cycles);
    Ok(config)
}

async fn fetch(addresses: Vec<String>) -> Result<BootstrapSnapshot, Box<dyn Error>> {
    if addresses.is_empty() || addresses.len() > 100 {
        return Err(
            io::Error::other("discovery requires 1..100 accounts per coherent request").into(),
        );
    }
    let config = BootstrapRpcConfig {
        rpc_url: env::var("HFT_SOLANA_RPC_URL")?,
        explicit_accounts: addresses,
        dlmm_bin_pairs: Vec::new(),
    };
    let snapshot = tokio::time::timeout(Duration::from_secs(30), fetch_bootstrap_snapshot(&config))
        .await
        .map_err(|_| io::Error::other("discovery RPC timed out"))?
        .map_err(|_| io::Error::other("discovery RPC failed; provider details omitted"))?;
    if snapshot.events.len() != snapshot.explicit_accounts || snapshot.events.iter().any(|event|
        !matches!(event, FeedEvent::Account(update) if update.is_startup && update.slot == snapshot.max_context_slot)) {
        return Err(io::Error::other("incomplete discovery RPC context").into());
    }
    Ok(snapshot)
}

fn write_json_new(path: &str, value: &Value) -> io::Result<()> {
    use std::io::Write;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    writeln!(output, "{}", serde_json::to_string_pretty(value)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let options = options(env::args().skip(1))?;
    let candidates: Candidates = serde_json::from_str(&fs::read_to_string(&options.candidates)?)?;
    if candidates.candidates.is_empty() || candidates.candidates.len() > 100 {
        return Err(io::Error::other("expected 1..100 candidates").into());
    }
    let mut addresses: Vec<_> = candidates
        .candidates
        .iter()
        .map(|c| c.address.clone())
        .collect();
    addresses.sort();
    addresses.dedup();
    let snapshot = fetch(addresses).await?;
    let accounts: BTreeMap<_, _> = snapshot
        .events
        .into_iter()
        .filter_map(|event| match event {
            FeedEvent::Account(update) => Some((key(update.pubkey), update)),
            _ => None,
        })
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let mut derived = Vec::new();
    for candidate in candidates.candidates {
        if !candidate.tvl.is_finite()
            || !candidate.volume_24h.is_finite()
            || candidate.tvl < 0.0
            || candidate.volume_24h < 0.0
        {
            return Err(io::Error::other("invalid candidate liquidity/activity metrics").into());
        }
        if let Some(account) = accounts.get(&candidate.address) {
            let address = candidate.address.clone();
            match derive(candidate, account, now) {
                Ok(pool) => derived.push(pool),
                Err(_) => println!(
                    "PAPER_DISCOVERY_REJECT pool={address} reason=owner_layout_mints_or_activation"
                ),
            }
        }
    }
    derived.sort_by(|a, b| a.candidate.address.cmp(&b.candidate.address));
    derived.dedup_by(|a, b| a.candidate.address == b.candidate.address);
    let template: Value = serde_json::from_str(&fs::read_to_string(&options.template)?)?;
    let mut activity_report = if options.activity_samples > 0 {
        // Orphans cannot form cycles, so spend no sampling requests on them.
        let mut counts = BTreeMap::new();
        for pool in &derived {
            let mut pair = pool.mints.clone();
            pair.sort();
            *counts.entry(pair).or_insert(0usize) += 1;
        }
        derived.retain(|pool| {
            let mut pair = pool.mints.clone();
            pair.sort();
            counts[&pair] >= 2
        });
        if derived.len() < 2 {
            return Err(io::Error::other(
                "not enough native connected candidates for activity sampling",
            )
            .into());
        }
        let report = measure_activity(&mut derived, &options).await?;
        derived.retain(|pool| pool.activity.compared_intervals > 0);
        Some(report)
    } else {
        None
    };
    let mut rejected = Vec::new();
    loop {
        let pools = select(&derived, options.maximum);
        if pools.len() < 2 {
            if let Some(report) = &mut activity_report {
                report["status"] = json!("no_qualified_active_pair");
                report["selected_pools"] = json!([]);
                report["rejected_after_sampling"] = json!(rejected);
                if let Some(path) = &options.activity_report {
                    write_json_new(path, report)?;
                }
                return Err(io::Error::other("no connected pair with observed activity passed discovery; refresh candidates or extend the bounded sampling window; no config written").into());
            }
        }
        let config = make_config(&template, &pools)?;
        let mut built = PaperConfig::from_json_str(&config.to_string())?.build()?;
        let snapshot = fetch(built.account_filters.clone()).await?;
        let mut invalid = Vec::new();
        // Discovery deliberately accepts only initialized legacy SPL mints.
        for event in &snapshot.events {
            if let FeedEvent::Account(update) = event {
                let address = key(update.pubkey);
                if config["tokens"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|mint| mint.as_str() == Some(address.as_str()))
                {
                    if key(update.owner) != TOKEN || inspect_mint(&update.data).is_err() {
                        for pool in &pools {
                            if pool.mints.contains(&address) {
                                invalid.push(pool.candidate.address.clone());
                            }
                        }
                    }
                }
            }
        }
        let result = apply_bootstrap_catchup(
            &mut built.pipeline,
            snapshot,
            std::iter::empty(),
            now * 1_000_000_000,
        );
        for (index, pool) in pools.iter().enumerate() {
            if !built.pipeline.store().is_ready(index as u32) {
                invalid.push(pool.candidate.address.clone());
            }
        }
        invalid.sort();
        invalid.dedup();
        if invalid.is_empty() {
            write_json_new(&options.output, &config)?;
            println!("PAPER_DISCOVERY_VALIDATED pools={} tokens={} cycles={} accounts={} context_slot={}", pools.len(), config["tokens"].as_array().unwrap().len(), config["cycles"].as_array().unwrap().len(), result.explicit_accounts, result.context_slot);
            for (index, pool) in pools.iter().enumerate() {
                println!("PAPER_DISCOVERY_POOL id={index} kind={} address={} index_tvl={} index_volume24h={} activity_changes={} activity_intervals={}", pool.candidate.kind, pool.candidate.address, pool.candidate.tvl, pool.candidate.volume_24h, pool.activity.changed_intervals, pool.activity.compared_intervals);
            }
            if let Some(report) = &mut activity_report {
                report["status"] = json!("validated");
                report["selected_pools"] = json!(pools
                    .iter()
                    .map(|p| &p.candidate.address)
                    .collect::<Vec<_>>());
                report["rejected_after_sampling"] = json!(rejected);
                report["validation_context_slot"] = json!(result.context_slot);
                if let Some(path) = &options.activity_report {
                    write_json_new(path, report)?;
                }
                println!("PAPER_ACTIVITY_RANKED sampled_pools={} selected_pools={} selected_with_changes={}", report["pools"].as_array().unwrap().len(), pools.len(), pools.iter().filter(|p| p.activity.changed_intervals > 0).count());
            }
            return Ok(());
        }
        for address in &invalid {
            println!(
                "PAPER_DISCOVERY_REJECT pool={address} reason=mint_safety_or_snapshot_not_ready"
            );
        }
        rejected.extend(invalid.iter().cloned());
        derived.retain(|pool| !invalid.contains(&pool.candidate.address));
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pool(address: &str, mints: [&str; 2]) -> DerivedPool {
        DerivedPool {
            candidate: Candidate {
                address: address.into(),
                kind: "raydium_cpmm".into(),
                mints: mints.map(str::to_owned),
                tvl: 1000.0,
                volume_24h: 100.0,
            },
            config: json!({"pool":address}),
            mints: mints.map(str::to_owned),
            activity: ActivityStats::default(),
        }
    }
    #[test]
    fn cycles_follow_reversed_mints_and_exclude_unconnected_pools() {
        let pools = vec![
            pool("a", [WSOL, "other"]),
            pool("b", ["other", WSOL]),
            pool("orphan", [WSOL, "lonely"]),
        ];
        let selected = select(&pools, 15);
        assert_eq!(selected.len(), 2);
        let config = make_config(
            &json!({"cycles":[{"search":{"minimum_probe":100}}]}),
            &selected,
        )
        .unwrap();
        assert_eq!(config["cycles"].as_array().unwrap().len(), 2);
        assert_eq!(config["cycles"][0]["edges"][0]["direction"], "a_to_b");
        assert_eq!(config["cycles"][0]["edges"][1]["direction"], "a_to_b");
        assert_eq!(config["tokens"][0], WSOL);
    }
    #[test]
    fn rejects_wrong_program_even_with_matching_pool_discriminator() {
        let address = Pubkey::new_from_array([1; 32]).to_string();
        let candidate = pool(&address, [WSOL, "other"]).candidate;
        let account = AccountUpdate {
            pubkey: [1; 32],
            owner: [2; 32],
            slot: 10,
            write_version: 0,
            generation: 0,
            bank_id: None,
            is_startup: true,
            data: vec![0; 637],
        };
        assert!(derive(candidate, &account, 100).is_err());
    }

    #[test]
    fn four_distinct_pools_form_twelve_directed_cycles() {
        let pools = vec![
            pool("a", [WSOL, "other"]),
            pool("b", [WSOL, "other"]),
            pool("c", ["other", WSOL]),
            pool("d", ["other", WSOL]),
        ];
        let config = make_config(
            &json!({"cycles":[{"search":{"minimum_probe":100}}]}),
            &pools,
        )
        .unwrap();
        let cycles = config["cycles"].as_array().unwrap();
        assert_eq!(cycles.len(), 12);
        assert!(cycles
            .iter()
            .all(|cycle| cycle["edges"][0]["pool"] != cycle["edges"][1]["pool"]));
    }

    #[test]
    fn recent_activity_beats_large_historical_volume_and_prefers_two_active_sides() {
        let mut pools = vec![
            pool("balanced-a", [WSOL, "balanced"]),
            pool("balanced-b", ["balanced", WSOL]),
            pool("one-fast", [WSOL, "one-side"]),
            pool("one-static", [WSOL, "one-side"]),
            pool("old-a", [WSOL, "historical"]),
            pool("old-b", [WSOL, "historical"]),
        ];
        for (index, pool) in pools.iter_mut().enumerate() {
            pool.activity = ActivityStats {
                changed_intervals: match index {
                    0 | 1 => 3,
                    2 => 12,
                    _ => 0,
                },
                compared_intervals: 12,
                first_context_slot: Some(10),
                ..Default::default()
            };
            if index >= 4 {
                pool.candidate.volume_24h = 1_000_000.0;
            }
        }
        let selected = select(&pools, 2);
        assert_eq!(
            selected
                .iter()
                .map(|p| p.candidate.address.as_str())
                .collect::<Vec<_>>(),
            vec!["balanced-a", "balanced-b"]
        );
    }

    #[test]
    fn disabled_sampling_retains_index_order() {
        let mut pools = vec![
            pool("low-a", [WSOL, "low"]),
            pool("low-b", [WSOL, "low"]),
            pool("high-a", [WSOL, "high"]),
            pool("high-b", [WSOL, "high"]),
        ];
        for pool in &mut pools[2..] {
            pool.candidate.volume_24h = 1000.0;
        }
        assert!(select(&pools, 2)
            .iter()
            .all(|p| p.mints.contains(&"high".to_owned())));
    }

    #[test]
    fn sampling_excludes_quiet_pairs_but_keeps_a_static_counterpart_to_an_active_pool() {
        let mut pools = vec![
            pool("active", [WSOL, "a"]),
            pool("counterpart", [WSOL, "a"]),
            pool("quiet-a", [WSOL, "b"]),
            pool("quiet-b", [WSOL, "b"]),
        ];
        for pool in &mut pools {
            pool.activity = ActivityStats {
                compared_intervals: 12,
                first_context_slot: Some(10),
                ..Default::default()
            };
        }
        pools[0].activity.changed_intervals = 1;
        let selected = select(&pools, 15);
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|p| p.mints.contains(&"a".to_owned())));
        pools[0].activity.changed_intervals = 0;
        assert!(select(&pools, 15).is_empty());
    }

    #[test]
    fn cli_bounds_activity_requests_and_requires_enabled_sampling_for_reports() {
        let parse = |tail: &[&str]| {
            options(
                ["candidates", "template", "new-output"]
                    .into_iter()
                    .chain(tail.iter().copied())
                    .map(str::to_owned),
            )
        };
        assert_eq!(parse(&[]).unwrap().maximum, 15);
        assert_eq!(
            parse(&["20", "--activity-samples", "13"])
                .unwrap()
                .activity_samples,
            13
        );
        for tail in [
            vec!["--activity-samples", "2"],
            vec!["--activity-samples", "62"],
            vec!["--activity-interval-secs", "0"],
            vec!["--activity-interval-secs", "31"],
            vec!["--activity-report", "new-report"],
            vec!["--unknown", "3"],
        ] {
            assert!(parse(&tail).is_err());
        }
    }
}
