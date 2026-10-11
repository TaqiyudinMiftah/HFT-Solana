//! Payload-change observations used only for discovery ranking, never quotes.
use std::{cmp::Ordering, io};

use hft_solana::feed::AccountUpdate;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct ActivityStats {
    pub compared_intervals: u64,
    pub changed_intervals: u64,
    pub skipped_contexts: u64,
    pub first_context_slot: Option<u64>,
    pub last_context_slot: Option<u64>,
}

impl ActivityStats {
    /// Compare exact fractions; more context advances alone cannot raise rank.
    pub fn compare_rate(&self, other: &Self) -> Ordering {
        let left = u128::from(self.changed_intervals) * u128::from(other.compared_intervals.max(1));
        let right =
            u128::from(other.changed_intervals) * u128::from(self.compared_intervals.max(1));
        left.cmp(&right)
    }
}

type Payload = ([u8; 32], [u8; 32], Vec<u8>);

#[derive(Default)]
pub struct ActivityTracker {
    pub stats: ActivityStats,
    previous: Option<Vec<Payload>>,
}

impl ActivityTracker {
    pub fn observe(&mut self, slot: u64, accounts: &[&AccountUpdate]) -> io::Result<()> {
        if accounts.len() != 3
            || accounts
                .iter()
                .any(|update| !update.is_startup || update.slot != slot)
        {
            return Err(io::Error::other(
                "activity requires one complete pool/vault context",
            ));
        }
        if self
            .stats
            .last_context_slot
            .is_some_and(|previous| slot <= previous)
        {
            // Discard stale/duplicate contexts without replacing the baseline.
            self.stats.skipped_contexts += 1;
            return Ok(());
        }
        let payloads: Vec<_> = accounts
            .iter()
            .map(|update| (update.pubkey, update.owner, update.data.clone()))
            .collect();
        if let Some(previous) = &self.previous {
            if previous.iter().zip(&payloads).any(|(a, b)| a.0 != b.0) {
                return Err(io::Error::other(
                    "activity watched account identity changed",
                ));
            }
            self.stats.compared_intervals += 1;
            if previous != &payloads {
                self.stats.changed_intervals += 1;
            }
        }
        self.stats.first_context_slot.get_or_insert(slot);
        self.stats.last_context_slot = Some(slot);
        self.previous = Some(payloads);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts(slot: u64, vault: u8) -> [AccountUpdate; 3] {
        std::array::from_fn(|i| AccountUpdate {
            pubkey: [i as u8; 32],
            owner: [9; 32],
            slot,
            write_version: slot,
            generation: slot,
            bank_id: None,
            is_startup: true,
            data: vec![if i == 2 { vault } else { 1 }],
        })
    }

    fn observe(tracker: &mut ActivityTracker, slot: u64, vault: u8) {
        let values = accounts(slot, vault);
        tracker
            .observe(slot, &values.iter().collect::<Vec<_>>())
            .unwrap();
    }

    #[test]
    fn counts_vault_changes_once_per_interval_not_context_or_write_version_changes() {
        let mut tracker = ActivityTracker::default();
        observe(&mut tracker, 10, 1);
        observe(&mut tracker, 11, 1);
        observe(&mut tracker, 12, 2);
        assert_eq!(tracker.stats.compared_intervals, 2);
        assert_eq!(tracker.stats.changed_intervals, 1);
    }

    #[test]
    fn old_and_duplicate_contexts_do_not_regress_the_baseline() {
        let mut tracker = ActivityTracker::default();
        observe(&mut tracker, 10, 1);
        observe(&mut tracker, 10, 2);
        observe(&mut tracker, 9, 2);
        observe(&mut tracker, 11, 1);
        assert_eq!(tracker.stats.compared_intervals, 1);
        assert_eq!(tracker.stats.changed_intervals, 0);
        assert_eq!(tracker.stats.skipped_contexts, 2);
    }

    #[test]
    fn refuses_partial_mixed_context_or_reordered_accounts() {
        let mut tracker = ActivityTracker::default();
        let mut values = accounts(10, 1);
        assert!(tracker.observe(10, &[&values[0], &values[1]]).is_err());
        values[2].slot = 11;
        assert!(tracker
            .observe(10, &values.iter().collect::<Vec<_>>())
            .is_err());
        observe(&mut tracker, 10, 1);
        values = accounts(11, 1);
        values.swap(0, 1);
        assert!(tracker
            .observe(11, &values.iter().collect::<Vec<_>>())
            .is_err());
        assert_eq!(tracker.stats.last_context_slot, Some(10));
    }

    #[test]
    fn ranks_by_observed_fraction_instead_of_raw_sample_count() {
        let fast = ActivityStats {
            changed_intervals: 2,
            compared_intervals: 3,
            ..Default::default()
        };
        let slow = ActivityStats {
            changed_intervals: 3,
            compared_intervals: 12,
            ..Default::default()
        };
        assert_eq!(fast.compare_rate(&slow), Ordering::Greater);
        assert_eq!(ActivityStats::default().compare_rate(&slow), Ordering::Less);
    }
}
