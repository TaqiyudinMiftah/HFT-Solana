use std::{collections::BTreeSet, str::FromStr};

use serde::Deserialize;
use solana_pubkey::Pubkey;
use thiserror::Error;

use crate::{
    active_store::ActivePoolStore,
    graph::{Cycle, Edge, GraphIndex},
    opportunity_engine::{CycleSearchConfig, OpportunityEngine},
    paper::PaperPipeline,
    reactor::{PaperStateReactor, PoolRecipe, PumpPoolRecipe, RaydiumPoolRecipe},
    types::{Direction, TokenId},
};

#[derive(Clone, Debug, Deserialize)]
pub struct PaperConfig {
    #[serde(default)]
    pub runtime: RuntimeConfig,
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
    pub account_filters: Vec<String>,
    pub feed_channel_capacity: usize,
    pub opportunity_channel_capacity: usize,
    pub filter_name: String,
}

#[derive(Debug, Error)]
pub enum PaperConfigError {
    #[error("invalid JSON config: {0}")]
    Json(#[from] serde_json::Error),
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
    #[error("cycle {cycle} references pool {pool}, but only {pool_count} pools exist")]
    PoolOutOfRange {
        cycle: usize,
        pool: u32,
        pool_count: usize,
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
        if self.pools.is_empty() {
            return Err(PaperConfigError::NoPools);
        }
        if self.cycles.is_empty() {
            return Err(PaperConfigError::NoCycles);
        }

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

        let pool_count = self.pools.len();
        let mut reactor = PaperStateReactor::new(
            self.runtime.max_versions_per_account,
            self.runtime.max_hot_slot_skew,
        );
        let mut subscribed = BTreeSet::<String>::new();

        for (index, pool) in self.pools.into_iter().enumerate() {
            let (recipe, accounts) = build_pool_recipe(pool)?;
            let pool_id = reactor.register(recipe);
            debug_assert_eq!(pool_id as usize, index);

            for account in accounts {
                subscribed.insert(Pubkey::new_from_array(account).to_string());
            }
        }

        let mut cycles = Vec::with_capacity(self.cycles.len());
        let mut configs = Vec::with_capacity(self.cycles.len());

        for (index, cycle) in self.cycles.into_iter().enumerate() {
            let (cycle, config) = build_cycle(index, pool_count, cycle)?;
            cycles.push(cycle);
            configs.push(config);
        }

        let graph = GraphIndex::from_cycles(pool_count, cycles);
        let engine = OpportunityEngine::new(graph, configs);
        let store = ActivePoolStore::new(pool_count, self.runtime.dirty_capacity);
        let pipeline = PaperPipeline::new(reactor, store, engine);

        Ok(BuiltPaperConfig {
            pipeline,
            account_filters: subscribed.into_iter().collect(),
            feed_channel_capacity: self.runtime.feed_channel_capacity,
            opportunity_channel_capacity: self.runtime.opportunity_channel_capacity,
            filter_name: self.runtime.filter_name,
        })
    }
}

fn parse_key(field: &'static str, value: String) -> Result<[u8; 32], PaperConfigError> {
    Pubkey::from_str(&value)
        .map(Pubkey::to_bytes)
        .map_err(|_| PaperConfigError::InvalidPubkey { field, value })
}

fn build_pool_recipe(
    pool: PoolConfig,
) -> Result<(PoolRecipe, Vec<[u8; 32]>), PaperConfigError> {
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
            ))
        }
    }
}

fn build_cycle(
    index: usize,
    pool_count: usize,
    config: CycleConfig,
) -> Result<(Cycle, CycleSearchConfig), PaperConfigError> {
    if !(2..=3).contains(&config.edges.len()) {
        return Err(PaperConfigError::InvalidCycleLength { cycle: index });
    }

    for edge in &config.edges {
        if edge.pool as usize >= pool_count {
            return Err(PaperConfigError::PoolOutOfRange {
                cycle: index,
                pool: edge.pool,
                pool_count,
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
        id: index
            .try_into()
            .expect("cycle registry exceeds u32"),
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
