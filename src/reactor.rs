use std::collections::{HashMap, HashSet};

use smallvec::SmallVec;

use crate::{
    feed::{AccountJournal, AccountUpdate, BankIdentity, FeedEvent, JournalApplyResult},
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

#[cfg(feature = "meteora-dlmm")]
#[derive(Clone, Debug)]
pub struct MeteoraDlmmRecipe {
    pub lb_pair: AccountKey,
    pub bin_arrays: Vec<AccountKey>,
    pub bitmap_extension: Option<AccountKey>,
    pub mint_x: AccountKey,
    pub mint_y: AccountKey,
    pub bin_array_take_count: u8,
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
    #[cfg(feature = "meteora-dlmm")]
    MeteoraDlmm(MeteoraDlmmRecipe),
}

impl PoolRecipe {
    fn dependencies(&self) -> SmallVec<[AccountKey; 16]> {
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
            #[cfg(feature = "meteora-dlmm")]
            PoolRecipe::MeteoraDlmm(recipe) => {
                let mut dependencies = SmallVec::new();
                dependencies.push(recipe.lb_pair);
                dependencies.extend(recipe.bin_arrays.iter().copied());
                if let Some(bitmap) = recipe.bitmap_extension {
                    dependencies.push(bitmap);
                }
                dependencies.push(recipe.mint_x);
                dependencies.push(recipe.mint_y);
                dependencies
            }
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
            #[cfg(feature = "meteora-dlmm")]
            PoolRecipe::MeteoraDlmm(_) => SmallVec::new(),
        }
    }

    fn is_slot_fenced(&self) -> bool {
        #[cfg(feature = "meteora-dlmm")]
        if matches!(self, PoolRecipe::MeteoraDlmm(_)) {
            return true;
        }

        false
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
    #[cfg(feature = "meteora-dlmm")]
    DlmmIncompleteBinWindow {
        missing: Vec<AccountKey>,
    },
    SlotFencePending {
        bank: BankIdentity,
    },
    SlotFenceSuperseded {
        fence: BankIdentity,
        dependency: AccountKey,
        current_generation: u64,
        current_slot: u64,
        current_bank_id: Option<u64>,
    },
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
    pending_fenced_banks: HashMap<BankIdentity, HashSet<PoolId>>,
    #[cfg(feature = "meteora-dlmm")]
    dlmm_pair_to_pools: HashMap<AccountKey, Vec<PoolId>>,
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
            pending_fenced_banks: HashMap::new(),
            #[cfg(feature = "meteora-dlmm")]
            dlmm_pair_to_pools: HashMap::new(),
        }
    }

    pub fn register(&mut self, recipe: PoolRecipe) -> PoolId {
        let pool_id: PoolId = self
            .recipes
            .len()
            .try_into()
            .expect("pool registry exceeds u32");

        #[cfg(feature = "meteora-dlmm")]
        if let PoolRecipe::MeteoraDlmm(dlmm) = &recipe {
            self.dlmm_pair_to_pools
                .entry(dlmm.lb_pair)
                .or_default()
                .push(pool_id);
        }

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
        match event {
            FeedEvent::Account(update) => self.process_account(update),
            FeedEvent::DiscardBanks { banks, .. } => self.process_discard_banks(&banks),
            FeedEvent::SlotComplete { bank } => self.process_slot_complete(bank),
        }
    }

    fn process_account(&mut self, update: AccountUpdate) -> Vec<ReactorOutput> {
        #[cfg(feature = "meteora-dlmm")]
        self.maybe_register_dlmm_bin_array(&update);

        let pubkey = update.pubkey;
        let bank = update.bank_id.map(|bank_id| BankIdentity {
            generation: update.generation,
            slot: update.slot,
            bank_id,
        });

        if self.journal.apply(update) == JournalApplyResult::Duplicate {
            return Vec::new();
        }

        let pool_ids = self.affected_pools([pubkey]);
        let mut outputs = Vec::new();

        for pool_id in pool_ids {
            let recipe = &self.recipes[pool_id as usize];
            if recipe.is_slot_fenced() {
                if let Some(bank) = bank.clone() {
                    self.pending_fenced_banks
                        .entry(bank.clone())
                        .or_default()
                        .insert(pool_id);

                    if self.ready[pool_id as usize] {
                        self.ready[pool_id as usize] = false;
                        outputs.push(ReactorOutput::PoolInvalidated {
                            pool_id,
                            reason: ReactorInvalidation::SlotFencePending { bank },
                        });
                    }
                    continue;
                }
            }

            self.rebuild_into(pool_id, None, &mut outputs);
        }

        outputs
    }

    #[cfg(feature = "meteora-dlmm")]
    fn maybe_register_dlmm_bin_array(&mut self, update: &AccountUpdate) {
        if update.owner != crate::decode::meteora_dlmm::DLMM_PROGRAM_ID_BYTES {
            return;
        }

        if self.account_to_pools.contains_key(&update.pubkey) {
            return;
        }

        let Ok(bin_array) = crate::decode::meteora_dlmm::decode_bin_array(&update.data) else {
            return;
        };
        let pair = bin_array.lb_pair.to_bytes();

        let Some(pool_ids) = self.dlmm_pair_to_pools.get(&pair).cloned() else {
            return;
        };

        for pool_id in pool_ids {
            let Some(PoolRecipe::MeteoraDlmm(recipe)) = self.recipes.get_mut(pool_id as usize)
            else {
                continue;
            };

            if !recipe.bin_arrays.contains(&update.pubkey) {
                recipe.bin_arrays.push(update.pubkey);
            }

            let pools = self.account_to_pools.entry(update.pubkey).or_default();
            if !pools.contains(&pool_id) {
                pools.push(pool_id);
            }
        }
    }

    fn process_discard_banks(&mut self, banks: &[BankIdentity]) -> Vec<ReactorOutput> {
        let discarded = self.journal.discard_banks(banks);
        let mut affected = self.affected_pools(discarded.changed_accounts);

        for bank in banks {
            if let Some(pools) = self.pending_fenced_banks.remove(bank) {
                affected.extend(pools);
            }
        }
        affected.sort_unstable();
        affected.dedup();

        let mut outputs = Vec::new();
        for pool_id in affected {
            if self.has_pending_fence(pool_id) {
                if self.ready[pool_id as usize] {
                    self.ready[pool_id as usize] = false;
                    outputs.push(ReactorOutput::PoolInvalidated {
                        pool_id,
                        reason: ReactorInvalidation::Snapshot(
                            "DLMM has a newer unfinished slot fence".to_owned(),
                        ),
                    });
                }
                continue;
            }

            self.rebuild_into(pool_id, None, &mut outputs);
        }

        outputs
    }

    fn process_slot_complete(&mut self, bank: BankIdentity) -> Vec<ReactorOutput> {
        let Some(pools) = self.pending_fenced_banks.remove(&bank) else {
            return Vec::new();
        };

        let mut pool_ids: Vec<_> = pools.into_iter().collect();
        pool_ids.sort_unstable();

        let mut outputs = Vec::new();
        for pool_id in pool_ids {
            // A newer bank may already be streaming. In that case the journal
            // can contain state beyond this fence, so wait for the newest
            // outstanding bank instead of publishing a mixed snapshot.
            if self.has_pending_fence(pool_id) {
                continue;
            }

            self.rebuild_into(pool_id, Some(&bank), &mut outputs);
        }

        outputs
    }

    fn affected_pools<I>(&self, accounts: I) -> Vec<PoolId>
    where
        I: IntoIterator<Item = AccountKey>,
    {
        let mut affected = HashSet::new();
        for account in accounts {
            if let Some(pools) = self.account_to_pools.get(&account) {
                affected.extend(pools.iter().copied());
            }
        }

        let mut pool_ids: Vec<_> = affected.into_iter().collect();
        pool_ids.sort_unstable();
        pool_ids
    }

    fn has_pending_fence(&self, pool_id: PoolId) -> bool {
        self.pending_fenced_banks
            .values()
            .any(|pools| pools.contains(&pool_id))
    }

    fn rebuild_into(
        &mut self,
        pool_id: PoolId,
        fence: Option<&BankIdentity>,
        outputs: &mut Vec<ReactorOutput>,
    ) {
        match self.rebuild(pool_id, fence) {
            Ok(state) => {
                self.ready[pool_id as usize] = true;
                outputs.push(ReactorOutput::PoolUpdated { pool_id, state });
            }
            Err(ReactorInvalidation::MissingDependency(_)) if !self.ready[pool_id as usize] => {}
            Err(reason) => {
                self.ready[pool_id as usize] = false;
                outputs.push(ReactorOutput::PoolInvalidated { pool_id, reason });
            }
        }
    }

    fn rebuild(
        &mut self,
        pool_id: PoolId,
        fence: Option<&BankIdentity>,
    ) -> Result<PoolState, ReactorInvalidation> {
        let recipe = self
            .recipes
            .get(pool_id as usize)
            .expect("registered pool id")
            .clone();

        let version = self.next_version(pool_id, &recipe, fence)?;

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
            #[cfg(feature = "meteora-dlmm")]
            PoolRecipe::MeteoraDlmm(recipe) => {
                let pair = self.require(&recipe.lb_pair)?;
                let mint_x = self.require(&recipe.mint_x)?;
                let mint_y = self.require(&recipe.mint_y)?;

                let mut bins = Vec::with_capacity(recipe.bin_arrays.len());
                for bin_key in &recipe.bin_arrays {
                    let account = self.require(bin_key)?;
                    bins.push((*bin_key, account.data.as_slice()));
                }

                let bitmap_data = match recipe.bitmap_extension {
                    Some(bitmap_key) => Some(self.require(&bitmap_key)?.data.as_slice()),
                    None => None,
                };

                crate::snapshot::assemble_meteora_dlmm_quote_state(
                    recipe.lb_pair,
                    &pair.data,
                    &bins,
                    bitmap_data,
                    recipe.mint_x,
                    mint_x.owner,
                    &mint_x.data,
                    recipe.mint_y,
                    mint_y.owner,
                    &mint_y.data,
                )
                .and_then(|quote| {
                    crate::snapshot::validate_meteora_dlmm_bin_window(
                        &quote,
                        recipe.bin_array_take_count,
                    )?;

                    Ok(PoolState::MeteoraDlmm(crate::state::MeteoraDlmmState {
                        version,
                        quote: std::sync::Arc::new(quote),
                    }))
                })
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
        fence: Option<&BankIdentity>,
    ) -> Result<StateVersion, ReactorInvalidation> {
        if recipe.is_slot_fenced() {
            return self.next_fenced_version(pool_id, recipe, fence);
        }

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
    fn next_fenced_version(
        &self,
        pool_id: PoolId,
        recipe: &PoolRecipe,
        fence: Option<&BankIdentity>,
    ) -> Result<StateVersion, ReactorInvalidation> {
        let mut causal_slot = 0u64;
        let mut causal_write_version = 0u64;

        for key in recipe.dependencies() {
            let account = self.require(&key)?;

            if let Some(fence) = fence {
                let is_future = account.generation > fence.generation
                    || (account.generation == fence.generation && account.slot > fence.slot);
                let same_slot_wrong_bank = account.generation == fence.generation
                    && account.slot == fence.slot
                    && account.bank_id.is_some()
                    && account.bank_id != Some(fence.bank_id);

                if is_future || same_slot_wrong_bank {
                    return Err(ReactorInvalidation::SlotFenceSuperseded {
                        fence: fence.clone(),
                        dependency: key,
                        current_generation: account.generation,
                        current_slot: account.slot,
                        current_bank_id: account.bank_id,
                    });
                }
            }

            causal_slot = causal_slot.max(account.slot);
            causal_write_version = causal_write_version.max(account.write_version);
        }

        Ok(StateVersion {
            slot: fence.map_or(causal_slot, |bank| bank.slot),
            write_version: causal_write_version,
            generation: self.local_generations[pool_id as usize].saturating_add(1),
        })
    }
}

fn snapshot_invalidation(error: SnapshotError) -> ReactorInvalidation {
    match error {
        #[cfg(feature = "meteora-dlmm")]
        SnapshotError::DlmmIncompleteBinWindow { missing } => {
            ReactorInvalidation::DlmmIncompleteBinWindow { missing }
        }
        other => ReactorInvalidation::Snapshot(other.to_string()),
    }
}
