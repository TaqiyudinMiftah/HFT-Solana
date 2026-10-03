use std::collections::{HashMap, HashSet};

use smallvec::SmallVec;

use crate::{
    feed::{AccountJournal, FeedEvent, JournalApplyResult},
    snapshot::{assemble_pump_state_auto, assemble_raydium_state_with_mints, SnapshotError},
    state::PoolState,
    types::{PoolId, StateVersion},
};

pub type AccountKey = [u8; 32];

#[derive(Clone, Debug)]
pub struct PumpPoolRecipe {
    pub pool: AccountKey,
    pub fee_config: AccountKey,
    pub pump_global: AccountKey,
    pub pump_amm_global_config: AccountKey,
    pub base_vault: AccountKey,
    pub quote_vault: AccountKey,
    pub base_mint: AccountKey,
    pub quote_mint: AccountKey,
}

#[cfg(feature = "meteora-damm")]
#[derive(Clone, Debug)]
pub struct MeteoraDammRecipe {
    pub pool: AccountKey,
    pub vault_a: AccountKey,
    pub vault_b: AccountKey,
    pub mint_a: AccountKey,
    pub mint_b: AccountKey,
}

#[derive(Clone, Debug)]
pub struct RaydiumPoolRecipe {
    pub pool: AccountKey,
    pub amm_config: AccountKey,
    pub vault_0: AccountKey,
    pub vault_1: AccountKey,
    pub mint_0: AccountKey,
    pub mint_1: AccountKey,
}

#[derive(Clone, Debug)]
pub enum PoolRecipe {
    Pump(PumpPoolRecipe),
    RaydiumCpmm(RaydiumPoolRecipe),
    #[cfg(feature = "meteora-damm")]
    MeteoraDamm(MeteoraDammRecipe),
}

impl PoolRecipe {
    fn dependencies(&self) -> SmallVec<[AccountKey; 8]> {
        match self {
            PoolRecipe::Pump(recipe) => SmallVec::from_slice(&[
                recipe.pool,
                recipe.fee_config,
                recipe.pump_global,
                recipe.pump_amm_global_config,
                recipe.base_vault,
                recipe.quote_vault,
                recipe.base_mint,
                recipe.quote_mint,
            ]),
            PoolRecipe::RaydiumCpmm(recipe) => SmallVec::from_slice(&[
                recipe.pool,
                recipe.amm_config,
                recipe.vault_0,
                recipe.vault_1,
                recipe.mint_0,
                recipe.mint_1,
            ]),
            #[cfg(feature = "meteora-damm")]
            PoolRecipe::MeteoraDamm(recipe) => SmallVec::from_slice(&[
                recipe.pool,
                recipe.vault_a,
                recipe.vault_b,
                recipe.mint_a,
                recipe.mint_b,
            ]),
        }
    }

    /// Accounts whose write slots should move together during a swap.
    ///
    /// Static configuration/mint accounts intentionally do not participate in
    /// this skew check: an old last-write slot does not make unchanged state
    /// stale.
    fn hot_dependencies(&self) -> SmallVec<[AccountKey; 3]> {
        match self {
            PoolRecipe::Pump(recipe) => {
                SmallVec::from_slice(&[recipe.base_vault, recipe.quote_vault])
            }
            PoolRecipe::RaydiumCpmm(recipe) => {
                SmallVec::from_slice(&[recipe.pool, recipe.vault_0, recipe.vault_1])
            }
            #[cfg(feature = "meteora-damm")]
            PoolRecipe::MeteoraDamm(recipe) => {
                SmallVec::from_slice(&[recipe.pool, recipe.vault_a, recipe.vault_b])
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactorInvalidation {
    MissingDependency(AccountKey),
    HotSlotSkew {
        min_slot: u64,
        max_slot: u64,
        max_allowed: u64,
    },
    Snapshot(String),
}

#[derive(Clone, Debug)]
pub enum ReactorOutput {
    PoolUpdated {
        pool_id: PoolId,
        state: PoolState,
    },
    PoolInvalidated {
        pool_id: PoolId,
        reason: ReactorInvalidation,
    },
}

pub struct PaperStateReactor {
    journal: AccountJournal,
    recipes: Vec<PoolRecipe>,
    account_to_pools: HashMap<AccountKey, Vec<PoolId>>,
    local_generations: Vec<u64>,
    ready: Vec<bool>,
    max_hot_slot_skew: u64,
}

impl PaperStateReactor {
    pub fn new(max_versions_per_account: usize, max_hot_slot_skew: u64) -> Self {
        Self {
            journal: AccountJournal::new(max_versions_per_account),
            recipes: Vec::new(),
            account_to_pools: HashMap::new(),
            local_generations: Vec::new(),
            ready: Vec::new(),
            max_hot_slot_skew,
        }
    }

    pub fn register(&mut self, recipe: PoolRecipe) -> PoolId {
        let pool_id: PoolId = self
            .recipes
            .len()
            .try_into()
            .expect("pool registry exceeds u32");

        let mut unique = HashSet::new();
        for dependency in recipe.dependencies() {
            if unique.insert(dependency) {
                self.account_to_pools
                    .entry(dependency)
                    .or_default()
                    .push(pool_id);
            }
        }

        self.recipes.push(recipe);
        self.local_generations.push(0);
        self.ready.push(false);
        pool_id
    }

    pub fn journal(&self) -> &AccountJournal {
        &self.journal
    }

    pub fn process(&mut self, event: FeedEvent) -> Vec<ReactorOutput> {
        let affected_accounts = match event {
            FeedEvent::Account(update) => {
                let pubkey = update.pubkey;
                if self.journal.apply(update) == JournalApplyResult::Duplicate {
                    return Vec::new();
                }
                vec![pubkey]
            }
            FeedEvent::DiscardBanks { banks, .. } => {
                let discarded = self.journal.discard_banks(&banks);
                discarded.changed_accounts
            }
        };

        let mut affected_pools = HashSet::new();
        for account in affected_accounts {
            if let Some(pools) = self.account_to_pools.get(&account) {
                affected_pools.extend(pools.iter().copied());
            }
        }

        let mut pool_ids: Vec<_> = affected_pools.into_iter().collect();
        pool_ids.sort_unstable();

        let mut outputs = Vec::with_capacity(pool_ids.len());
        for pool_id in pool_ids {
            match self.rebuild(pool_id) {
                Ok(state) => {
                    self.ready[pool_id as usize] = true;
                    outputs.push(ReactorOutput::PoolUpdated { pool_id, state });
                }
                Err(ReactorInvalidation::MissingDependency(_)) if !self.ready[pool_id as usize] => {
                }
                Err(reason) => {
                    self.ready[pool_id as usize] = false;
                    outputs.push(ReactorOutput::PoolInvalidated { pool_id, reason });
                }
            }
        }

        outputs
    }

    fn rebuild(&mut self, pool_id: PoolId) -> Result<PoolState, ReactorInvalidation> {
        let recipe = self
            .recipes
            .get(pool_id as usize)
            .expect("registered pool id")
            .clone();

        let version = self.next_version(pool_id, &recipe)?;

        let state = match recipe {
            PoolRecipe::Pump(recipe) => {
                let pool = self.require(&recipe.pool)?;
                let fee_config = self.require(&recipe.fee_config)?;
                let pump_global = self.require(&recipe.pump_global)?;
                let amm_global = self.require(&recipe.pump_amm_global_config)?;
                let base_vault = self.require(&recipe.base_vault)?;
                let quote_vault = self.require(&recipe.quote_vault)?;
                let base_mint = self.require(&recipe.base_mint)?;
                let quote_mint = self.require(&recipe.quote_mint)?;

                assemble_pump_state_auto(
                    &pool.data,
                    &fee_config.data,
                    &pump_global.data,
                    &amm_global.data,
                    &base_vault.data,
                    &quote_vault.data,
                    &base_mint.data,
                    &quote_mint.data,
                    version,
                )
                .map(|(state, _)| PoolState::Pump(state))
            }
            PoolRecipe::RaydiumCpmm(recipe) => {
                let pool = self.require(&recipe.pool)?;
                let amm_config = self.require(&recipe.amm_config)?;
                let vault_0 = self.require(&recipe.vault_0)?;
                let vault_1 = self.require(&recipe.vault_1)?;
                let mint_0 = self.require(&recipe.mint_0)?;
                let mint_1 = self.require(&recipe.mint_1)?;

                assemble_raydium_state_with_mints(
                    &pool.data,
                    &amm_config.data,
                    &vault_0.data,
                    &vault_1.data,
                    &mint_0.data,
                    &mint_1.data,
                    version,
                )
                .map(PoolState::RaydiumCpmm)
            }
            #[cfg(feature = "meteora-damm")]
            PoolRecipe::MeteoraDamm(recipe) => {
                let pool = self.require(&recipe.pool)?;
                let vault_a = self.require(&recipe.vault_a)?;
                let vault_b = self.require(&recipe.vault_b)?;
                let mint_a = self.require(&recipe.mint_a)?;
                let mint_b = self.require(&recipe.mint_b)?;

                crate::snapshot::assemble_meteora_damm_state_with_mints(
                    &pool.data,
                    &vault_a.data,
                    &vault_b.data,
                    &mint_a.data,
                    &mint_b.data,
                    version,
                )
                .map(PoolState::MeteoraDamm)
            }
        };

        let state = state.map_err(snapshot_invalidation)?;
        self.local_generations[pool_id as usize] = version.generation;
        Ok(state)
    }

    fn require(
        &self,
        pubkey: &AccountKey,
    ) -> Result<&crate::feed::AccountUpdate, ReactorInvalidation> {
        self.journal
            .current(pubkey)
            .ok_or(ReactorInvalidation::MissingDependency(*pubkey))
    }

    fn next_version(
        &self,
        pool_id: PoolId,
        recipe: &PoolRecipe,
    ) -> Result<StateVersion, ReactorInvalidation> {
        let hot = recipe.hot_dependencies();
        let mut hot_min_slot = u64::MAX;
        let mut hot_max_slot = 0u64;

        for key in hot {
            let account = self.require(&key)?;
            hot_min_slot = hot_min_slot.min(account.slot);
            hot_max_slot = hot_max_slot.max(account.slot);
        }

        if hot_max_slot.saturating_sub(hot_min_slot) > self.max_hot_slot_skew {
            return Err(ReactorInvalidation::HotSlotSkew {
                min_slot: hot_min_slot,
                max_slot: hot_max_slot,
                max_allowed: self.max_hot_slot_skew,
            });
        }

        // The skew gate intentionally considers only accounts that must move
        // together during swaps. The published causal version, however, must
        // cover every dependency so a newer fee/config/pool update cannot be
        // represented with an older slot/write-version pair.
        let mut causal_slot = 0u64;
        let mut causal_write_version = 0u64;
        for key in recipe.dependencies() {
            let account = self.require(&key)?;
            causal_slot = causal_slot.max(account.slot);
            causal_write_version = causal_write_version.max(account.write_version);
        }

        Ok(StateVersion {
            slot: causal_slot,
            write_version: causal_write_version,
            generation: self.local_generations[pool_id as usize].saturating_add(1),
        })
    }
}

fn snapshot_invalidation(error: SnapshotError) -> ReactorInvalidation {
    ReactorInvalidation::Snapshot(error.to_string())
}
