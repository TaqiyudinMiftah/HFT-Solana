use hft_solana::{
    decode::mint::{extension_is_quote_safe, inspect_mint},
    snapshot::MintQuoteSafety,
};
use spl_token_2022_interface::extension::ExtensionType;

fn legacy_mint(supply: u64, decimals: u8) -> Vec<u8> {
    let mut data = vec![0u8; 82];

    // mint_authority: COption::None
    data[..4].copy_from_slice(&0u32.to_le_bytes());
    data[36..44].copy_from_slice(&supply.to_le_bytes());
    data[44] = decimals;
    data[45] = 1;

    // freeze_authority: COption::None
    data[46..50].copy_from_slice(&0u32.to_le_bytes());

    data
}

#[test]
fn legacy_mint_is_safe_and_supply_is_available_for_fee_tiers() {
    let info = inspect_mint(&legacy_mint(1_000_000_000, 6)).unwrap();

    assert_eq!(info.supply, 1_000_000_000);
    assert_eq!(info.decimals, 6);
    assert_eq!(info.extension_count, 0);
    assert_eq!(info.safety, MintQuoteSafety::Safe);
}

#[test]
fn blocks_extensions_that_change_transfer_execution_or_amount() {
    assert!(!extension_is_quote_safe(ExtensionType::TransferFeeConfig));
    assert!(!extension_is_quote_safe(ExtensionType::TransferHook));
    assert!(!extension_is_quote_safe(ExtensionType::NonTransferable));
    assert!(!extension_is_quote_safe(ExtensionType::Pausable));
    assert!(!extension_is_quote_safe(
        ExtensionType::ConfidentialTransferMint
    ));
}

#[test]
fn allows_extensions_that_do_not_change_raw_transfer_economics() {
    assert!(extension_is_quote_safe(ExtensionType::MetadataPointer));
    assert!(extension_is_quote_safe(ExtensionType::InterestBearingConfig));
    assert!(extension_is_quote_safe(ExtensionType::ScaledUiAmount));
    assert!(extension_is_quote_safe(ExtensionType::PermanentDelegate));
}
