use hft_solana::feed::{AccountJournal, AccountUpdate, BankIdentity, JournalApplyResult};

fn update(
    pubkey: u8,
    generation: u64,
    slot: u64,
    bank_id: Option<u64>,
    write_version: u64,
    data: u8,
) -> AccountUpdate {
    AccountUpdate {
        pubkey: [pubkey; 32],
        owner: [9; 32],
        slot,
        write_version,
        generation,
        bank_id,
        is_startup: bank_id.is_none(),
        data: vec![data],
    }
}

#[test]
fn discarded_bank_rolls_back_to_previous_account_version() {
    let mut journal = AccountJournal::new(8);
    let key = [1u8; 32];

    journal.apply(update(1, 1, 100, None, 1, 10));
    journal.apply(update(1, 1, 101, Some(7), 2, 20));
    journal.apply(update(1, 1, 102, Some(8), 3, 30));

    assert_eq!(journal.current(&key).unwrap().data, vec![30]);

    let result = journal.discard_banks(&[BankIdentity {
        generation: 1,
        slot: 102,
        bank_id: 8,
    }]);

    assert_eq!(result.changed_accounts, vec![key]);
    assert!(result.missing_fallback.is_empty());
    assert_eq!(journal.current(&key).unwrap().data, vec![20]);
}

#[test]
fn discard_reports_missing_fallback_instead_of_hiding_it() {
    let mut journal = AccountJournal::new(4);
    let key = [2u8; 32];

    journal.apply(update(2, 4, 200, Some(99), 1, 55));

    let result = journal.discard_banks(&[BankIdentity {
        generation: 4,
        slot: 200,
        bank_id: 99,
    }]);

    assert_eq!(result.changed_accounts, vec![key]);
    assert_eq!(result.missing_fallback, vec![key]);
    assert!(journal.current(&key).is_none());
}

#[test]
fn same_version_is_deduplicated_or_replaced() {
    let mut journal = AccountJournal::new(4);

    let original = update(3, 1, 300, Some(3), 9, 1);
    assert_eq!(
        journal.apply(original.clone()),
        JournalApplyResult::Inserted
    );
    assert_eq!(journal.apply(original), JournalApplyResult::Duplicate);

    assert_eq!(
        journal.apply(update(3, 1, 300, Some(3), 9, 2)),
        JournalApplyResult::ReplacedSameVersion
    );
    assert_eq!(journal.current(&[3; 32]).unwrap().data, vec![2]);
}

#[test]
fn bounded_history_keeps_latest_versions() {
    let mut journal = AccountJournal::new(2);

    journal.apply(update(4, 1, 1, Some(1), 1, 1));
    journal.apply(update(4, 1, 2, Some(2), 2, 2));
    journal.apply(update(4, 1, 3, Some(3), 3, 3));

    assert_eq!(journal.versions(&[4; 32]), 2);
    assert_eq!(journal.current(&[4; 32]).unwrap().data, vec![3]);
}
