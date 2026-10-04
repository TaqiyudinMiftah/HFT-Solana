#![cfg(feature = "meteora-dlmm")]

use std::str::FromStr;

use hft_solana::{
    feed::{AccountUpdate, BankIdentity, FeedEvent},
    quote::meteora_dlmm::quote_exact_in_at_slot,
    reactor::{
        MeteoraDlmmRecipe, PaperStateReactor, PoolRecipe, ReactorInvalidation, ReactorOutput,
    },
    state::PoolState,
    types::Direction,
};
use solana_sdk_v2::pubkey::Pubkey;

const LB_PAIR: &str = "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd";
const BIN_ARRAY_1: &str = "338HBraHxVupeftangX6jySecbND4osxcJjjMSW7qmMs";
const BIN_ARRAY_2: &str = "28BX6QycwTKx3CqswpJQs7hJCmoUs469Qt4maKMdhgmQ";
const TOKEN_X_MINT: &str = "BBZU4HYvY4qMGE5MbWsVxGweGBZJqGRsgH8tAEAKusNk";
const TOKEN_Y_MINT: &str = "31iVdsS8fkURXg737XQwYhXVAuGv2vNYHjyDiURStkaU";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

fn bytes(name: &str) -> &'static [u8] {
    match name {
        "pair" => include_bytes!(concat!(
            "../fixtures/meteora_dlmm/",
            "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/lb_pair.bin"
        )),
        "bin1" => include_bytes!(concat!(
            "../fixtures/meteora_dlmm/",
            "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/bin_array_1.bin"
        )),
        "bin2" => include_bytes!(concat!(
            "../fixtures/meteora_dlmm/",
            "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/bin_array_2.bin"
        )),
        "mint_x" => include_bytes!(concat!(
            "../fixtures/meteora_dlmm/",
            "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/token_x_mint.bin"
        )),
        "mint_y" => include_bytes!(concat!(
            "../fixtures/meteora_dlmm/",
            "9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd/token_y_mint.bin"
        )),
        _ => unreachable!(),
    }
}

fn key(value: &str) -> [u8; 32] {
    Pubkey::from_str(value).unwrap().to_bytes()
}

fn update(
    pubkey: [u8; 32],
    owner: [u8; 32],
    slot: u64,
    write_version: u64,
    bank_id: Option<u64>,
    data: &[u8],
) -> FeedEvent {
    FeedEvent::Account(AccountUpdate {
        pubkey,
        owner,
        slot,
        write_version,
        generation: 1,
        bank_id,
        is_startup: bank_id.is_none(),
        data: data.to_vec(),
    })
}

fn setup_reactor() -> PaperStateReactor {
    let mut reactor = PaperStateReactor::new(8, 0);
    reactor.register(PoolRecipe::MeteoraDlmm(MeteoraDlmmRecipe {
        lb_pair: key(LB_PAIR),
        bin_arrays: vec![key(BIN_ARRAY_1), key(BIN_ARRAY_2)],
        bitmap_extension: None,
        mint_x: key(TOKEN_X_MINT),
        mint_y: key(TOKEN_Y_MINT),
    }));
    reactor
}

fn bootstrap(reactor: &mut PaperStateReactor) -> ReactorOutput {
    let token_program = key(TOKEN_PROGRAM);
    let dummy_owner = [77u8; 32];

    for event in [
        update(key(LB_PAIR), dummy_owner, 100, 1, None, bytes("pair")),
        update(key(BIN_ARRAY_1), dummy_owner, 100, 2, None, bytes("bin1")),
        update(key(BIN_ARRAY_2), dummy_owner, 100, 3, None, bytes("bin2")),
        update(
            key(TOKEN_X_MINT),
            token_program,
            90,
            4,
            None,
            bytes("mint_x"),
        ),
    ] {
        assert!(reactor.process(event).is_empty());
    }

    let output = reactor.process(update(
        key(TOKEN_Y_MINT),
        token_program,
        90,
        5,
        None,
        bytes("mint_y"),
    ));
    assert_eq!(output.len(), 1);
    output.into_iter().next().unwrap()
}

#[test]
fn dlmm_waits_for_slot_complete_and_allows_untouched_old_bins() {
    let mut reactor = setup_reactor();
    let initial = bootstrap(&mut reactor);

    match initial {
        ReactorOutput::PoolUpdated { state, .. } => {
            assert!(matches!(state, PoolState::MeteoraDlmm(_)));
        }
        ReactorOutput::PoolInvalidated { .. } => panic!("startup should become ready"),
    }

    let bank = BankIdentity {
        generation: 1,
        slot: 600,
        bank_id: 77,
    };
    let dummy_owner = [77u8; 32];

    let first = reactor.process(update(
        key(BIN_ARRAY_1),
        dummy_owner,
        bank.slot,
        10,
        Some(bank.bank_id),
        bytes("bin1"),
    ));
    assert!(matches!(
        first.as_slice(),
        [ReactorOutput::PoolInvalidated {
            reason: ReactorInvalidation::SlotFencePending { bank: pending },
            ..
        }] if pending == &bank
    ));

    // The pair is part of the same bank. BinArray #2 is intentionally
    // untouched and remains at startup slot 100.
    assert!(reactor
        .process(update(
            key(LB_PAIR),
            dummy_owner,
            bank.slot,
            11,
            Some(bank.bank_id),
            bytes("pair"),
        ))
        .is_empty());

    let completed = reactor.process(FeedEvent::SlotComplete { bank: bank.clone() });
    assert_eq!(completed.len(), 1);

    let state = match &completed[0] {
        ReactorOutput::PoolUpdated { state, .. } => state,
        ReactorOutput::PoolInvalidated { reason, .. } => {
            panic!("slot fence should publish coherent DLMM state: {reason:?}")
        }
    };

    let PoolState::MeteoraDlmm(state) = state else {
        panic!("expected DLMM state");
    };

    assert_eq!(state.version.slot, 600);
    assert_eq!(state.version.generation, 2);
    assert_eq!(state.quote.bin_arrays.len(), 2);

    let quote = quote_exact_in_at_slot(
        state.quote.as_ref(),
        40_000_000_000,
        Direction::AtoB,
        1_753_751_761,
        state.quote.lb_pair.activation_point.saturating_add(1),
        state.version,
    )
    .unwrap();
    assert!(quote.amount_out > 0);
}

#[test]
fn older_slot_complete_does_not_publish_over_newer_pending_bank() {
    let mut reactor = setup_reactor();
    bootstrap(&mut reactor);

    let dummy_owner = [77u8; 32];
    let bank_a = BankIdentity {
        generation: 1,
        slot: 700,
        bank_id: 80,
    };
    let bank_b = BankIdentity {
        generation: 1,
        slot: 701,
        bank_id: 81,
    };

    assert_eq!(
        reactor
            .process(update(
                key(BIN_ARRAY_1),
                dummy_owner,
                bank_a.slot,
                20,
                Some(bank_a.bank_id),
                bytes("bin1"),
            ))
            .len(),
        1
    );
    assert!(reactor
        .process(update(
            key(BIN_ARRAY_2),
            dummy_owner,
            bank_b.slot,
            21,
            Some(bank_b.bank_id),
            bytes("bin2"),
        ))
        .is_empty());

    assert!(reactor
        .process(FeedEvent::SlotComplete {
            bank: bank_a.clone(),
        })
        .is_empty());

    let completed = reactor.process(FeedEvent::SlotComplete {
        bank: bank_b.clone(),
    });
    assert_eq!(completed.len(), 1);
    assert!(matches!(
        &completed[0],
        ReactorOutput::PoolUpdated { state, .. } if state.version().slot == bank_b.slot
    ));
}
