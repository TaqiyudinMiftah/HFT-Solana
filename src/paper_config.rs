use std::{collections::BTreeSet, str::FromStr};

use serde::Deserialize;
use solana_pubkey::Pubkey;
use thiserror::Error;

use crate::{
    active_store::ActivePoolStore,
    graph::{Cycle, Edge, GraphIndex},
    landing::{LandingCandidate, LandingPolicyConfig, LandingProvider},
    opportunity_engine::{CycleSearchConfig, OpportunityEngine},
    paper::PaperPipeline,
    reactor::{
        MeteoraDammRecipe, MeteoraDlmmRecipe, PaperStateReactor, PoolRecipe, PumpPoolRecipe,
        RaydiumPoolRecipe,
    },
    types::{Direction, TokenId},
};

#[derive(Clone, Debug, Deserialize)]
pub struct PaperConfig {
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub landing: Option<LandingConfig>,
    pub tokens: Vec<String>,
    pub pools: Vec<PoolConfig>,
    pub cycles: Vec<CycleConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub max_versions_per_account: usize,
    pub max_hot_slot_skew: u64,
    pub dirty_capacity: usize,
    pub feed_channel_capacity: usize,
    pub opportunity_channel_capacity: usize,
    pub filter_name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LandingConfig {
    pub base_fee: u64,
    pub minimum_net_if_landed: i128,
    pub minimum_expected_value: i128,
    pub max_tip_share_bps: u16,
    pub candidates: Vec<LandingCandidateConfig>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct LandingCandidateConfig {
    pub provider: LandingProviderConfig,
    pub success_probability_bps: u16,
    pub priority_fee: u64,
    #[serde(default)]
    pub relay_tip: u64,
    #[serde(default)]
    pub relay_tip_share_bps: Option<u16>,
    #[serde(default)]
    pub minimum_relay_tip: u64,
    #[serde(default)]
    pub maximum_relay_tip: Option<u64>,
    pub failure_fee: u64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandingProviderConfig {
    Direct,
    Jito,
    HeliusSender,
}

impl From<LandingProviderConfig> for LandingProvider {
    fn from(value: LandingProviderConfig) -> Self {
        match value {
            LandingProviderConfig::Direct => LandingProvider::Direct,
            LandingProviderConfig::Jito => LandingProvider::Jito,
            LandingProviderConfig::HeliusSender => LandingProvider::HeliusSender,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuiltLandingPolicy {
    pub config: LandingPolicyConfig,
    pub candidates: Vec<LandingCandidate>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_versions_per_account: 8,
            max_hot_slot_skew: 1,
            dirty_capacity: 8_192,
            feed_channel_capacity: 8_192,
            opportunity_channel_capacity: 2_048,
            filter_name: "hft-paper".to_string(),
        }
    }
}

fn default_dlmm_bin_array_take_count() -> u8 {
    2
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PoolConfig {
    Pump {
        pool: String,
        fee_config: String,
        pump_global: String,
        pump_amm_global_config: String,
        base_vault: String,
        quote_vault: String,
        base_mint: String,
        quote_mint: String,
    },
    RaydiumCpmm {
        pool: String,
        amm_config: String,
        vault_0: String,
        vault_1: String,
        mint_0: String,
        mint_1: String,
    },
    MeteoraDamm {
        pool: String,
        vault_a: String,
        vault_b: String,
        mint_a: String,
        mint_b: String,
    },
    MeteoraDlmm {
        lb_pair: String,
        bin_arrays: Vec<String>,
        bitmap_extension: Option<String>,
        mint_x: String,
        mint_y: String,
        #[serde(default = "default_dlmm_bin_array_take_count")]
        bin_array_take_count: u8,
    },
}

#[derive(Clone, Debug, Deserialize)]
pub struct CycleConfig {
    pub start_token: TokenId,
    pub edges: Vec<EdgeConfig>,
    pub search: SearchConfig,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct EdgeConfig {
    pub pool: u32,
    pub direction: DirectionConfig,
    pub from_token: TokenId,
    pub to_token: TokenId,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectionConfig {
    AToB,
    BToA,
}

impl From<DirectionConfig> for Direction {
    fn from(value: DirectionConfig) -> Self {
        match value {
            DirectionConfig::AToB => Direction::AtoB,
            DirectionConfig::BToA => Direction::BtoA,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct SearchConfig {
    pub initial_seed: u64,
    pub minimum_probe: u64,
    pub max_size: u64,
    pub minimum_effective_profit: i128,
    pub max_slot_skew: u64,
    pub expected_cu: u32,
}

pub struct BuiltPaperConfig {
    pub pipeline: PaperPipeline,
    pub landing: Option<BuiltLandingPolicy>,
    pub account_filters: Vec<String>,
    pub dlmm_bin_pairs: Vec<[u8; 32]>,
    pub feed_channel_capacity: usize,
    pub opportunity_channel_capacity: usize,
    pub filter_name: String,
}

#[derive(Debug, Error)]
pub enum PaperConfigError {
    #[error("invalid JSON config: {0}")]
    Json(#[from] serde_json::Error),
    #[error("config must contain at least one token")]
    NoTokens,
    #[error("config must contain at least one pool")]
    NoPools,
    #[error("config must contain at least one cycle")]
    NoCycles,
    #[error("runtime capacity {field} must be nonzero")]
    ZeroCapacity { field: &'static str },
    #[error("invalid pubkey in {field}: {value}")]
    InvalidPubkey { field: &'static str, value: String },
    #[error("cycle {cycle} must contain 2 or 3 edges")]
    InvalidCycleLength { cycle: usize },
    #[error("Meteora DLMM pool must configure at least one bin array")]
    DlmmNoBinArrays,
    #[error("Meteora DLMM bin_array_take_count must be nonzero")]
    DlmmZeroBinArrayTakeCount,
    #[error("landing policy must contain at least one candidate")]
    LandingNoCandidates,
    #[error("landing max_tip_share_bps must be <= 10000, got {value}")]
    LandingTipShareOutOfRange { value: u16 },
    #[error("landing candidate {candidate} success_probability_bps must be <= 10000, got {value}")]
    LandingProbabilityOutOfRange { candidate: usize, value: u16 },
    #[error("landing candidate {candidate} relay_tip_share_bps must be <= 10000, got {value}")]
    LandingTipShareOutOfRange { candidate: usize, value: u16 },
    #[error("landing candidate {candidate} minimum_relay_tip exceeds maximum_relay_tip")]
    LandingTipClampInvalid { candidate: usize },
    #[error("cycle {cycle} references pool {pool}, but only {pool_count} pools exist")]
    PoolOutOfRange {
        cycle: usize,
        pool: u32,
        pool_count: usize,
    },
    #[error(
        "cycle {cycle} edge {edge} references token {token}, but only {token_count} tokens exist"
    )]
    TokenOutOfRange {
        cycle: usize,
        edge: usize,
        token: TokenId,
        token_count: usize,
    },
    #[error("cycle {cycle} edge {edge} direction does not match pool {pool} mint pair")]
    PoolMintMismatch {
        cycle: usize,
        edge: usize,
        pool: u32,
    },
    #[error("cycle {cycle} is not token-contiguous at edge {edge}")]
    TokenDiscontinuity { cycle: usize, edge: usize },
    #[error("cycle {cycle} does not return to start token {start_token}")]
    CycleNotClosed { cycle: usize, start_token: TokenId },
}

impl PaperConfig {
    pub fn from_json_str(json: &str) -> Result<Self, PaperConfigError> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn build(self) -> Result<BuiltPaperConfig, PaperConfigError> {
        if self.tokens.is_empty() {
            return Err(PaperConfigError::NoTokens);
        }
        if self.pools.is_empty() {
            return Err(PaperConfigError::NoPools);
        }
        if self.cycles.is_empty() {
            return Err(PaperConfigError::NoCycles);
        }

        let landing = self.landing.map(build_landing_policy).transpose()?;

        for (field, value) in [
            ("dirty_capacity", self.runtime.dirty_capacity),
            ("feed_channel_capacity", self.runtime.feed_channel_capacity),
            (
                "opportunity_channel_capacity",
                self.runtime.opportunity_channel_capacity,
            ),
            (
                "max_versions_per_account",
                self.runtime.max_versions_per_account,
            ),
        ] {
            if value == 0 {
                return Err(PaperConfigError::ZeroCapacity { field });
            }
        }

        let token_mints = self
            .tokens
            .into_iter()
            .map(|value| parse_key("tokens[]", value))
            .collect::<Result<Vec<_>, _>>()?;

        let pool_count = self.pools.len();
        let mut reactor = PaperStateReactor::new(
            self.runtime.max_versions_per_account,
            self.runtime.max_hot_slot_skew,
        );
        let mut subscribed = BTreeSet::<String>::new();
        let mut dlmm_bin_pairs = BTreeSet::<[u8; 32]>::new();
        let mut pool_topologies = Vec::with_capacity(pool_count);

        for (index, pool) in self.pools.into_iter().enumerate() {
            let (recipe, accounts, topology) = build_pool_recipe(pool)?;

            if let PoolRecipe::MeteoraDlmm(recipe) = &recipe {
                dlmm_bin_pairs.insert(recipe.lb_pair);
            }

            let pool_id = reactor.register(recipe);
            debug_assert_eq!(pool_id as usize, index);

            for account in accounts {
                subscribed.insert(Pubkey::new_from_array(account).to_string());
            }
            pool_topologies.push(topology);
        }

        let mut cycles = Vec::with_capacity(self.cycles.len());
        let mut configs = Vec::with_capacity(self.cycles.len());

        for (index, cycle) in self.cycles.into_iter().enumerate() {
            let (cycle, config) =
                build_cycle(index, pool_count, &token_mints, &pool_topologies, cycle)?;
            cycles.push(cycle);
            configs.push(config);
        }

        let graph = GraphIndex::from_cycles(pool_count, cycles);
        let engine = OpportunityEngine::new(graph, configs);
        let store = ActivePoolStore::new(pool_count, self.runtime.dirty_capacity);
        let pipeline = PaperPipeline::new(reactor, store, engine);

        Ok(BuiltPaperConfig {
            pipeline,
            landing,
            account_filters: subscribed.into_iter().collect(),
            dlmm_bin_pairs: dlmm_bin_pairs.into_iter().collect(),
            feed_channel_capacity: self.runtime.feed_channel_capacity,
            opportunity_channel_capacity: self.runtime.opportunity_channel_capacity,
            filter_name: self.runtime.filter_name,
        })
    }
}

fn parse_key(field: &'static str, value: String) -> Result<[u8; 32], PaperConfigError> {
    Pubkey::from_str(&value)
        .map(|key| key.to_bytes())
        .map_err(|_| PaperConfigError::InvalidPubkey { field, value })
}

#[derive(Clone, Copy, Debug)]
struct PoolTopology {
    mint_a: [u8; 32],
    mint_b: [u8; 32],
}

fn build_landing_policy(config: LandingConfig) -> Result<BuiltLandingPolicy, PaperConfigError> {
    if config.candidates.is_empty() {
        return Err(PaperConfigError::LandingNoCandidates);
    }
    if config.max_tip_share_bps > 10_000 {
        return Err(PaperConfigError::LandingTipShareOutOfRange {
            value: config.max_tip_share_bps,
        });
    }

    let candidates = config
        .candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| {
            if candidate.success_probability_bps > 10_000 {
                return Err(PaperConfigError::LandingProbabilityOutOfRange {
                    candidate: index,
                    value: candidate.success_probability_bps,
                });
            }

            if let Some(share_bps) = candidate.relay_tip_share_bps {
                if share_bps > 10_000 {
                    return Err(PaperConfigError::LandingTipShareOutOfRange {
                        candidate: index,
                        value: share_bps,
                    });
                }
            }
            if candidate
                .maximum_relay_tip
                .is_some_and(|maximum| candidate.minimum_relay_tip > maximum)
            {
                return Err(PaperConfigError::LandingTipClampInvalid { candidate: index });
            }

            Ok(LandingCandidate {
                provider: candidate.provider.into(),
                success_probability_bps: candidate.success_probability_bps,
                priority_fee: candidate.priority_fee,
                relay_tip: candidate.relay_tip,
                relay_tip_share_bps: candidate.relay_tip_share_bps,
                minimum_relay_tip: candidate.minimum_relay_tip,
                maximum_relay_tip: candidate.maximum_relay_tip,
                failure_fee: candidate.failure_fee,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(BuiltLandingPolicy {
        config: LandingPolicyConfig {
            base_fee: config.base_fee,
            minimum_net_if_landed: config.minimum_net_if_landed,
            minimum_expected_value: config.minimum_expected_value,
            max_tip_share_bps: config.max_tip_share_bps,
        },
        candidates,
    })
}

fn build_pool_recipe(
    pool: PoolConfig,
) -> Result<(PoolRecipe, Vec<[u8; 32]>, PoolTopology), PaperConfigError> {
    match pool {
        PoolConfig::Pump {
            pool,
            fee_config,
            pump_global,
            pump_amm_global_config,
            base_vault,
            quote_vault,
            base_mint,
            quote_mint,
        } => {
            let pool = parse_key("pools[].pool", pool)?;
            let fee_config = parse_key("pools[].fee_config", fee_config)?;
            let pump_global = parse_key("pools[].pump_global", pump_global)?;
            let pump_amm_global_config =
                parse_key("pools[].pump_amm_global_config", pump_amm_global_config)?;
            let base_vault = parse_key("pools[].base_vault", base_vault)?;
            let quote_vault = parse_key("pools[].quote_vault", quote_vault)?;
            let base_mint = parse_key("pools[].base_mint", base_mint)?;
            let quote_mint = parse_key("pools[].quote_mint", quote_mint)?;

            let accounts = vec![
                pool,
                fee_config,
                pump_global,
                pump_amm_global_config,
                base_vault,
                quote_vault,
                base_mint,
                quote_mint,
            ];

            Ok((
                PoolRecipe::Pump(PumpPoolRecipe {
                    pool,
                    fee_config,
                    pump_global,
                    pump_amm_global_config,
                    base_vault,
                    quote_vault,
                    base_mint,
                    quote_mint,
                }),
                accounts,
                PoolTopology {
                    mint_a: base_mint,
                    mint_b: quote_mint,
                },
            ))
        }
        PoolConfig::RaydiumCpmm {
            pool,
            amm_config,
            vault_0,
            vault_1,
            mint_0,
            mint_1,
        } => {
            let pool = parse_key("pools[].pool", pool)?;
            let amm_config = parse_key("pools[].amm_config", amm_config)?;
            let vault_0 = parse_key("pools[].vault_0", vault_0)?;
            let vault_1 = parse_key("pools[].vault_1", vault_1)?;
            let mint_0 = parse_key("pools[].mint_0", mint_0)?;
            let mint_1 = parse_key("pools[].mint_1", mint_1)?;

            let accounts = vec![pool, amm_config, vault_0, vault_1, mint_0, mint_1];

            Ok((
                PoolRecipe::RaydiumCpmm(RaydiumPoolRecipe {
                    pool,
                    amm_config,
                    vault_0,
                    vault_1,
                    mint_0,
                    mint_1,
                }),
                accounts,
                PoolTopology {
                    mint_a: mint_0,
                    mint_b: mint_1,
                },
            ))
        }
        PoolConfig::MeteoraDamm {
            pool,
            vault_a,
            vault_b,
            mint_a,
            mint_b,
        } => {
            let pool = parse_key("pools[].pool", pool)?;
            let vault_a = parse_key("pools[].vault_a", vault_a)?;
            let vault_b = parse_key("pools[].vault_b", vault_b)?;
            let mint_a = parse_key("pools[].mint_a", mint_a)?;
            let mint_b = parse_key("pools[].mint_b", mint_b)?;

            let accounts = vec![pool, vault_a, vault_b, mint_a, mint_b];

            Ok((
                PoolRecipe::MeteoraDamm(MeteoraDammRecipe {
                    pool,
                    vault_a,
                    vault_b,
                    mint_a,
                    mint_b,
                }),
                accounts,
                PoolTopology { mint_a, mint_b },
            ))
        }
        PoolConfig::MeteoraDlmm {
            lb_pair,
            bin_arrays,
            bitmap_extension,
            mint_x,
            mint_y,
            bin_array_take_count,
        } => {
            if bin_arrays.is_empty() {
                return Err(PaperConfigError::DlmmNoBinArrays);
            }
            if bin_array_take_count == 0 {
                return Err(PaperConfigError::DlmmZeroBinArrayTakeCount);
            }

            let lb_pair = parse_key("pools[].lb_pair", lb_pair)?;
            let bin_arrays = bin_arrays
                .into_iter()
                .map(|value| parse_key("pools[].bin_arrays[]", value))
                .collect::<Result<Vec<_>, _>>()?;
            let bitmap_extension = bitmap_extension
                .map(|value| parse_key("pools[].bitmap_extension", value))
                .transpose()?;
            let mint_x = parse_key("pools[].mint_x", mint_x)?;
            let mint_y = parse_key("pools[].mint_y", mint_y)?;

            let mut accounts = Vec::with_capacity(
                1 + bin_arrays.len() + usize::from(bitmap_extension.is_some()) + 2,
            );
            accounts.push(lb_pair);
            accounts.extend(bin_arrays.iter().copied());
            if let Some(bitmap) = bitmap_extension {
                accounts.push(bitmap);
            }
            accounts.push(mint_x);
            accounts.push(mint_y);

            Ok((
                PoolRecipe::MeteoraDlmm(MeteoraDlmmRecipe {
                    lb_pair,
                    bin_arrays,
                    bitmap_extension,
                    mint_x,
                    mint_y,
                    bin_array_take_count,
                }),
                accounts,
                PoolTopology {
                    mint_a: mint_x,
                    mint_b: mint_y,
                },
            ))
        }
    }
}

fn build_cycle(
    index: usize,
    pool_count: usize,
    token_mints: &[[u8; 32]],
    pool_topologies: &[PoolTopology],
    config: CycleConfig,
) -> Result<(Cycle, CycleSearchConfig), PaperConfigError> {
    if !(2..=3).contains(&config.edges.len()) {
        return Err(PaperConfigError::InvalidCycleLength { cycle: index });
    }

    for (edge_index, edge) in config.edges.iter().enumerate() {
        if edge.pool as usize >= pool_count {
            return Err(PaperConfigError::PoolOutOfRange {
                cycle: index,
                pool: edge.pool,
                pool_count,
            });
        }

        let from_mint =
            token_mints
                .get(edge.from_token as usize)
                .ok_or(PaperConfigError::TokenOutOfRange {
                    cycle: index,
                    edge: edge_index,
                    token: edge.from_token,
                    token_count: token_mints.len(),
                })?;
        let to_mint =
            token_mints
                .get(edge.to_token as usize)
                .ok_or(PaperConfigError::TokenOutOfRange {
                    cycle: index,
                    edge: edge_index,
                    token: edge.to_token,
                    token_count: token_mints.len(),
                })?;
        let topology = &pool_topologies[edge.pool as usize];

        let (expected_from, expected_to) = match edge.direction {
            DirectionConfig::AToB => (&topology.mint_a, &topology.mint_b),
            DirectionConfig::BToA => (&topology.mint_b, &topology.mint_a),
        };

        if from_mint != expected_from || to_mint != expected_to {
            return Err(PaperConfigError::PoolMintMismatch {
                cycle: index,
                edge: edge_index,
                pool: edge.pool,
            });
        }
    }

    if config.edges[0].from_token != config.start_token {
        return Err(PaperConfigError::TokenDiscontinuity {
            cycle: index,
            edge: 0,
        });
    }

    for edge_index in 1..config.edges.len() {
        if config.edges[edge_index - 1].to_token != config.edges[edge_index].from_token {
            return Err(PaperConfigError::TokenDiscontinuity {
                cycle: index,
                edge: edge_index,
            });
        }
    }

    if config.edges.last().expect("validated non-empty").to_token != config.start_token {
        return Err(PaperConfigError::CycleNotClosed {
            cycle: index,
            start_token: config.start_token,
        });
    }

    let converted: Vec<Edge> = config
        .edges
        .iter()
        .map(|edge| Edge {
            pool: edge.pool,
            direction: edge.direction.into(),
            from_token: edge.from_token,
            to_token: edge.to_token,
        })
        .collect();

    let filler = converted[0];
    let mut edges = [filler; 3];
    edges[..converted.len()].copy_from_slice(&converted);

    let cycle = Cycle {
        id: index.try_into().expect("cycle registry exceeds u32"),
        len: converted.len() as u8,
        edges,
        start_token: config.start_token,
    };

    let search = CycleSearchConfig {
        initial_seed: config.search.initial_seed,
        minimum_probe: config.search.minimum_probe,
        max_size: config.search.max_size,
        minimum_effective_profit: config.search.minimum_effective_profit,
        max_slot_skew: config.search.max_slot_skew,
        expected_cu: config.search.expected_cu,
    };

    Ok((cycle, search))
}
