//! Validate public index candidates using RPC and the existing state decoders.
use std::{collections::BTreeMap, env, error::Error, fs, io, str::FromStr, time::Duration};

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
    })
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
        .filter(|group| group.len() >= 2)
        .collect();
    groups.sort_by(|a, b| {
        let score =
            |group: &[DerivedPool]| group.iter().map(|p| p.candidate.volume_24h).sum::<f64>();
        score(b)
            .total_cmp(&score(a))
            .then_with(|| a[0].candidate.address.cmp(&b[0].candidate.address))
    });
    let mut result = Vec::new();
    for mut group in groups {
        group.sort_by(|a, b| {
            b.candidate
                .volume_24h
                .total_cmp(&a.candidate.volume_24h)
                .then_with(|| b.candidate.tvl.total_cmp(&a.candidate.tvl))
                .then_with(|| a.candidate.address.cmp(&b.candidate.address))
        });
        let take = group.len().min(4).min(maximum.saturating_sub(result.len()));
        if take >= 2 {
            result.extend(group.into_iter().take(take));
        }
    }
    result
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err(io::Error::other("usage: paper_discover <candidates.json> <template.json> <new-config.json> [max-pools=15]").into());
    }
    if std::path::Path::new(&args[2]).exists() {
        return Err(io::Error::other("output exists; choose a new config path").into());
    }
    let maximum: usize = args.get(3).map(String::as_str).unwrap_or("15").parse()?;
    if !(2..=20).contains(&maximum) {
        return Err(io::Error::other("max-pools must be 2..20").into());
    }
    let candidates: Candidates = serde_json::from_str(&fs::read_to_string(&args[0])?)?;
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
    let template: Value = serde_json::from_str(&fs::read_to_string(&args[1])?)?;
    loop {
        let pools = select(&derived, maximum);
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
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[2])?;
            use std::io::Write;
            writeln!(output, "{}", serde_json::to_string_pretty(&config)?)?;
            println!("PAPER_DISCOVERY_VALIDATED pools={} tokens={} cycles={} accounts={} context_slot={}", pools.len(), config["tokens"].as_array().unwrap().len(), config["cycles"].as_array().unwrap().len(), result.explicit_accounts, result.context_slot);
            for (index, pool) in pools.iter().enumerate() {
                println!("PAPER_DISCOVERY_POOL id={index} kind={} address={} index_tvl={} index_volume24h={}", pool.candidate.kind, pool.candidate.address, pool.candidate.tvl, pool.candidate.volume_24h);
            }
            return Ok(());
        }
        for address in &invalid {
            println!(
                "PAPER_DISCOVERY_REJECT pool={address} reason=mint_safety_or_snapshot_not_ready"
            );
        }
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
}
