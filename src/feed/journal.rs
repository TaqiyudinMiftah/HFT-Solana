use std::collections::{HashMap, HashSet, VecDeque};

use super::{AccountUpdate, BankIdentity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalApplyResult {
    Inserted,
    ReplacedSameVersion,
    Duplicate,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JournalDiscardResult {
    /// Accounts whose visible latest raw state changed after removing fork data.
    pub changed_accounts: Vec<[u8; 32]>,
    /// Accounts whose discarded latest state had no retained fallback.
    ///
    /// A state reactor should treat these as requiring re-bootstrap rather
    /// than silently continuing with missing liquidity.
    pub missing_fallback: Vec<[u8; 32]>,
}

#[derive(Debug)]
pub struct AccountJournal {
    max_versions_per_account: usize,
    accounts: HashMap<[u8; 32], VecDeque<AccountUpdate>>,
}

impl AccountJournal {
    pub fn new(max_versions_per_account: usize) -> Self {
        assert!(
            max_versions_per_account > 0,
            "account journal history must be nonzero"
        );

        Self {
            max_versions_per_account,
            accounts: HashMap::new(),
        }
    }

    pub fn apply(&mut self, update: AccountUpdate) -> JournalApplyResult {
        let history = self.accounts.entry(update.pubkey).or_default();

        if let Some(existing) = history.iter_mut().find(|entry| {
            entry.generation == update.generation
                && entry.slot == update.slot
                && entry.bank_id == update.bank_id
                && entry.write_version == update.write_version
        }) {
            if *existing == update {
                return JournalApplyResult::Duplicate;
            }

            *existing = update;
            return JournalApplyResult::ReplacedSameVersion;
        }

        history.push_back(update);
        while history.len() > self.max_versions_per_account {
            history.pop_front();
        }

        JournalApplyResult::Inserted
    }

    pub fn current(&self, pubkey: &[u8; 32]) -> Option<&AccountUpdate> {
        self.accounts.get(pubkey)?.back()
    }

    pub fn versions(&self, pubkey: &[u8; 32]) -> usize {
        self.accounts.get(pubkey).map_or(0, VecDeque::len)
    }

    pub fn discard_banks(&mut self, banks: &[BankIdentity]) -> JournalDiscardResult {
        if banks.is_empty() {
            return JournalDiscardResult::default();
        }

        let discarded: HashSet<(u64, u64, u64)> = banks
            .iter()
            .map(|bank| (bank.generation, bank.slot, bank.bank_id))
            .collect();

        let mut result = JournalDiscardResult::default();
        let mut remove_accounts = Vec::new();

        for (pubkey, history) in self.accounts.iter_mut() {
            let old_current = history.back().map(version_identity);
            let old_current_discarded = history
                .back()
                .map(|entry| entry_matches_discard(entry, &discarded))
                .unwrap_or(false);

            history.retain(|entry| !entry_matches_discard(entry, &discarded));

            let new_current = history.back().map(version_identity);
            if old_current != new_current {
                result.changed_accounts.push(*pubkey);
            }

            if old_current_discarded && history.is_empty() {
                result.missing_fallback.push(*pubkey);
                remove_accounts.push(*pubkey);
            }
        }

        for pubkey in remove_accounts {
            self.accounts.remove(&pubkey);
        }

        result.changed_accounts.sort_unstable();
        result.missing_fallback.sort_unstable();
        result
    }
}

type VersionIdentity = (u64, u64, Option<u64>, u64);

fn version_identity(update: &AccountUpdate) -> VersionIdentity {
    (
        update.generation,
        update.slot,
        update.bank_id,
        update.write_version,
    )
}

fn entry_matches_discard(update: &AccountUpdate, discarded: &HashSet<(u64, u64, u64)>) -> bool {
    let Some(bank_id) = update.bank_id else {
        // Startup snapshots are not bank-scoped and are retained as fallback.
        return false;
    };

    discarded.contains(&(update.generation, update.slot, bank_id))
}
