use hft_solana::{
    decode::raydium::{AMM_CONFIG_DISCRIMINATOR, POOL_STATE_DISCRIMINATOR},
    feed::{AccountUpdate, BankIdentity, FeedEvent},
    reactor::{
        PaperStateReactor, PoolRecipe, RaydiumPoolRecipe, ReactorInvalidation, ReactorOutput,
    },
    state::PoolState,
};

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn legacy_mint(supply: u64) -> Vec<u8> {
    let mut data = vec![0u8; 82];
    data[..4].copy_from_slice(&0u32.to_le_bytes());
    put_u64(&mut data, 36, supply);
    data[44] = 6;
    data[45] = 1;
    data[46..50].copy_from_slice(&0u32.to_le_bytes());
    data
}

fn token_account(mint: [u8; 32], amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; 165];
    data[..32].copy_from_slice(&mint);
    put_u64(&mut data, 64, amount);
    data[108] = 1;
    data
}

fn raydium_pool(mint_0: [u8; 32], mint_1: [u8; 32]) -> Vec<u8> {
    let mut data = vec![0u8; 413];
    data[..8].copy_from_slice(&POOL_STATE_DISCRIMINATOR);
    data[168..200].copy_from_slice(&mint_0);
    data[200..232].copy_from_slice(&mint_1);
    data[389] = 0;
    data
}

fn raydium_config() -> Vec<u8> {
    let mut data = vec![0u8; 116];
    data[..8].copy_from_slice(&AMM_CONFIG_DISCRIMINATOR);
    put_u64(&mut data, 12, 2_500);
    put_u64(&mut data, 20, 120_000);
    put_u64(&mut data, 28, 40_000);
    data
}

fn update(
    key: [u8; 32],
    slot: u64,
    write_version: u64,
    bank_id: Option<u64>,
    data: Vec<u8>,
) -> FeedEvent {
    FeedEvent::Account(AccountUpdate {
        pubkey: key,
        owner: [99; 32],
        slot,
        write_version,
        generation: 1,
        bank_id,
        is_startup: bank_id.is_none(),
        data,
    })
}

fn updated_slot(output: &[ReactorOutput]) -> Option<u64> {
    match output {
        [ReactorOutput::PoolUpdated { state, .. }] => Some(state.version().slot),
        _ => None,
    }
}

#[test]
fn reactor_waits_for_coherent_hot_accounts_and_rolls_back_discarded_bank() {
    let pool_key = [1u8; 32];
    let config_key = [2u8; 32];
    let vault_0_key = [3u8; 32];
    let vault_1_key = [4u8; 32];
    let mint_0_key = [5u8; 32];
    let mint_1_key = [6u8; 32];

    let mut reactor = PaperStateReactor::new(8, 0);
    let pool_id = reactor.register(PoolRecipe::RaydiumCpmm(RaydiumPoolRecipe {
        pool: pool_key,
        amm_config: config_key,
        vault_0: vault_0_key,
        vault_1: vault_1_key,
        mint_0: mint_0_key,
        mint_1: mint_1_key,
    }));
    assert_eq!(pool_id, 0);

    let mint_0 = [11u8; 32];
    let mint_1 = [12u8; 32];

    // Static dependencies can have old last-write slots.
    assert!(reactor
        .process(update(config_key, 10, 1, None, raydium_config()))
        .is_empty());
    assert!(reactor
        .process(update(mint_0_key, 20, 1, None, legacy_mint(1_000_000)))
        .is_empty());
    assert!(reactor
        .process(update(mint_1_key, 20, 1, None, legacy_mint(1_000_000)))
        .is_empty());

    // Initial coherent hot state at slot 100.
    assert!(reactor
        .process(update(
            pool_key,
            100,
            1,
            Some(10),
            raydium_pool(mint_0, mint_1),
        ))
        .is_empty());
    assert!(reactor
        .process(update(
            vault_0_key,
            100,
            2,
            Some(10),
            token_account(mint_0, 1_000_000),
        ))
        .is_empty());

    let initial = reactor.process(update(
        vault_1_key,
        100,
        3,
        Some(10),
        token_account(mint_1, 2_000_000),
    ));
    assert_eq!(updated_slot(&initial), Some(100));

    // A new swap arrives account-by-account. Until all hot accounts reach the
    // new bank slot the old quote state must not stay marked valid.
    let first = reactor.process(update(
        vault_0_key,
        101,
        4,
        Some(11),
        token_account(mint_0, 1_010_000),
    ));
    assert!(matches!(
        first.as_slice(),
        [ReactorOutput::PoolInvalidated {
            reason: ReactorInvalidation::HotSlotSkew { .. },
            ..
        }]
    ));

    let second = reactor.process(update(
        vault_1_key,
        101,
        5,
        Some(11),
        token_account(mint_1, 1_980_000),
    ));
    assert!(matches!(
        second.as_slice(),
        [ReactorOutput::PoolInvalidated {
            reason: ReactorInvalidation::HotSlotSkew { .. },
            ..
        }]
    ));

    let coherent = reactor.process(update(
        pool_key,
        101,
        6,
        Some(11),
        raydium_pool(mint_0, mint_1),
    ));
    assert_eq!(updated_slot(&coherent), Some(101));

    // Reconnect/fork recovery discards bank 11. All three hot accounts roll
    // back to retained slot-100 versions and the pool is rebuilt there.
    let rollback = reactor.process(FeedEvent::DiscardBanks {
        banks: vec![BankIdentity {
            generation: 1,
            slot: 101,
            bank_id: 11,
        }],
        reason: "test fork".to_owned(),
    });

    assert_eq!(updated_slot(&rollback), Some(100));

    match &rollback[0] {
        ReactorOutput::PoolUpdated { state, .. } => match state {
            PoolState::RaydiumCpmm(state) => {
                assert_eq!(state.reserve_a, 1_000_000);
                assert_eq!(state.reserve_b, 2_000_000);
            }
            PoolState::Pump(_) => panic!("unexpected Pump state"),
            #[cfg(feature = "meteora-damm")]
            PoolState::MeteoraDamm(_) => panic!("unexpected Meteora DAMM state"),
            #[cfg(feature = "meteora-dlmm")]
            PoolState::MeteoraDlmm(_) => panic!("unexpected Meteora DLMM state"),
        },
        ReactorOutput::PoolInvalidated { .. } => panic!("rollback should rebuild"),
    }
}

#[test]
fn static_dependency_update_advances_causal_version_without_hot_skew_failure() {
    let pool_key = [21u8; 32];
    let config_key = [22u8; 32];
    let vault_0_key = [23u8; 32];
    let vault_1_key = [24u8; 32];
    let mint_0_key = [25u8; 32];
    let mint_1_key = [26u8; 32];

    let mut reactor = PaperStateReactor::new(8, 0);
    reactor.register(PoolRecipe::RaydiumCpmm(RaydiumPoolRecipe {
        pool: pool_key,
        amm_config: config_key,
        vault_0: vault_0_key,
        vault_1: vault_1_key,
        mint_0: mint_0_key,
        mint_1: mint_1_key,
    }));

    let mint_0 = [31u8; 32];
    let mint_1 = [32u8; 32];

    assert!(reactor
        .process(update(config_key, 10, 1, None, raydium_config()))
        .is_empty());
    assert!(reactor
        .process(update(mint_0_key, 10, 1, None, legacy_mint(1_000_000)))
        .is_empty());
    assert!(reactor
        .process(update(mint_1_key, 10, 1, None, legacy_mint(1_000_000)))
        .is_empty());
    assert!(reactor
        .process(update(
            pool_key,
            100,
            1,
            Some(20),
            raydium_pool(mint_0, mint_1),
        ))
        .is_empty());
    assert!(reactor
        .process(update(
            vault_0_key,
            100,
            2,
            Some(20),
            token_account(mint_0, 1_000_000),
        ))
        .is_empty());

    let initial = reactor.process(update(
        vault_1_key,
        100,
        3,
        Some(20),
        token_account(mint_1, 2_000_000),
    ));
    assert_eq!(updated_slot(&initial), Some(100));

    // Only static config moves. Hot accounts are still coherent at slot 100,
    // so the pool remains valid, but its causal version must advance to 105.
    let changed = reactor.process(update(config_key, 105, 9, Some(21), raydium_config()));
    assert_eq!(updated_slot(&changed), Some(105));

    match &changed[0] {
        ReactorOutput::PoolUpdated { state, .. } => {
            assert_eq!(state.version().write_version, 9);
            assert_eq!(state.version().generation, 2);
        }
        ReactorOutput::PoolInvalidated { .. } => panic!("static config update should rebuild"),
    }
}
