#![cfg(feature = "meteora-damm")]

use hft_solana::{
    decode::meteora_damm::{POOL_ACCOUNT_LEN, POOL_DISCRIMINATOR},
    snapshot::{assemble_meteora_damm_state_with_mints, SnapshotError},
    types::StateVersion,
};
use meteora_cp_amm::{
    get_initial_pool_information,
    state::{CollectFeeMode, Pool},
    InitialPoolInformation,
};
use meteora_damm_sdk::calculate_initial_sqrt_price::calculate_compounding_initial_sqrt_price_and_liquidity;
use solana_pubkey::Pubkey;

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn legacy_mint(supply: u64, decimals: u8) -> Vec<u8> {
    let mut data = vec![0u8; 82];
    data[..4].copy_from_slice(&0u32.to_le_bytes());
    put_u64(&mut data, 36, supply);
    data[44] = decimals;
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

fn compounding_pool(token_a_amount: u64, token_b_amount: u64) -> Pool {
    let (sqrt_price, liquidity) =
        calculate_compounding_initial_sqrt_price_and_liquidity(token_a_amount, token_b_amount)
            .unwrap();

    let InitialPoolInformation {
        token_a_amount,
        token_b_amount,
        sqrt_price,
        ..
    } = get_initial_pool_information(
        CollectFeeMode::Compounding,
        0,
        0,
        sqrt_price,
        liquidity,
    )
    .unwrap();

    Pool {
        collect_fee_mode: CollectFeeMode::Compounding.into(),
        token_a_amount,
        token_b_amount,
        liquidity,
        sqrt_price,
        layout_version: 1,
        ..Default::default()
    }
}

fn pool_account_bytes(pool: &Pool) -> Vec<u8> {
    let mut data = vec![0u8; POOL_ACCOUNT_LEN];
    data[..8].copy_from_slice(&POOL_DISCRIMINATOR);
    data[8..].copy_from_slice(bytemuck::bytes_of(pool));
    data
}

fn version() -> StateVersion {
    StateVersion {
        slot: 500,
        write_version: 7,
        generation: 3,
    }
}

#[test]
fn raw_accounts_assemble_official_meteora_state() {
    let mint_a = Pubkey::new_from_array([1u8; 32]);
    let mint_b = Pubkey::new_from_array([2u8; 32]);

    let mut pool = compounding_pool(1_000_000_000, 2_000_000_000);
    pool.token_a_mint = mint_a;
    pool.token_b_mint = mint_b;
    pool.token_a_vault = Pubkey::new_from_array([3u8; 32]);
    pool.token_b_vault = Pubkey::new_from_array([4u8; 32]);

    let state = assemble_meteora_damm_state_with_mints(
        &pool_account_bytes(&pool),
        &token_account(mint_a.to_bytes(), pool.token_a_amount),
        &token_account(mint_b.to_bytes(), pool.token_b_amount),
        &legacy_mint(1_000_000_000_000, 6),
        &legacy_mint(2_000_000_000_000, 6),
        version(),
    )
    .unwrap();

    assert_eq!(state.version, version());
    assert_eq!(state.pool.token_a_mint, mint_a);
    assert_eq!(state.pool.token_b_mint, mint_b);
    assert_eq!(state.pool.token_a_amount, pool.token_a_amount);
    assert_eq!(state.pool.token_b_amount, pool.token_b_amount);
    assert_eq!(state.pool.liquidity, pool.liquidity);
    assert_eq!(state.pool.sqrt_price, pool.sqrt_price);
    assert_eq!(state.pool.collect_fee_mode, pool.collect_fee_mode);
}

#[test]
fn snapshot_rejects_vault_mint_mismatch() {
    let mint_a = Pubkey::new_from_array([11u8; 32]);
    let mint_b = Pubkey::new_from_array([12u8; 32]);
    let wrong_mint = Pubkey::new_from_array([13u8; 32]);

    let mut pool = compounding_pool(1_000_000, 2_000_000);
    pool.token_a_mint = mint_a;
    pool.token_b_mint = mint_b;

    let error = assemble_meteora_damm_state_with_mints(
        &pool_account_bytes(&pool),
        &token_account(wrong_mint.to_bytes(), pool.token_a_amount),
        &token_account(mint_b.to_bytes(), pool.token_b_amount),
        &legacy_mint(1_000_000_000, 6),
        &legacy_mint(2_000_000_000, 6),
        version(),
    )
    .unwrap_err();

    assert_eq!(error, SnapshotError::VaultMintMismatch);
}
