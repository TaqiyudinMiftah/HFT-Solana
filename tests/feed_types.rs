use hft_solana::feed::{AccountUpdate, BankIdentity, FeedEvent};

#[test]
fn feed_events_preserve_bank_identity() {
    let event = FeedEvent::DiscardBanks {
        banks: vec![BankIdentity {
            generation: 7,
            slot: 100,
            bank_id: 55,
        }],
        reason: "reconnect".to_owned(),
    };

    match event {
        FeedEvent::DiscardBanks { banks, reason } => {
            assert_eq!(banks[0].generation, 7);
            assert_eq!(banks[0].slot, 100);
            assert_eq!(banks[0].bank_id, 55);
            assert_eq!(reason, "reconnect");
        }
        FeedEvent::Account(_) | FeedEvent::SlotComplete { .. } => {
            panic!("unexpected feed event")
        }
    }
}

#[test]
fn account_update_carries_version_inputs() {
    let update = AccountUpdate {
        pubkey: [1; 32],
        owner: [2; 32],
        slot: 10,
        write_version: 20,
        generation: 30,
        bank_id: Some(40),
        is_startup: false,
        data: vec![1, 2, 3],
    };

    assert_eq!(update.slot, 10);
    assert_eq!(update.write_version, 20);
    assert_eq!(update.generation, 30);
    assert_eq!(update.bank_id, Some(40));
}

#[test]
fn slot_complete_preserves_exact_bank_identity() {
    let event = FeedEvent::SlotComplete {
        bank: BankIdentity {
            generation: 8,
            slot: 101,
            bank_id: 56,
        },
    };

    match event {
        FeedEvent::SlotComplete { bank } => {
            assert_eq!(bank.generation, 8);
            assert_eq!(bank.slot, 101);
            assert_eq!(bank.bank_id, 56);
        }
        _ => panic!("unexpected feed event"),
    }
}
