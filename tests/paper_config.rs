#![cfg(feature = "yellowstone")]

use hft_solana::paper_config::{PaperConfig, PaperConfigError};
use serde_json::json;
use solana_pubkey::Pubkey;

fn key(byte: u8) -> String {
    Pubkey::new_from_array([byte; 32]).to_string()
}

fn config_json() -> String {
    json!({
        "runtime": {
            "max_versions_per_account": 4,
            "max_hot_slot_skew": 1,
            "dirty_capacity": 16,
            "feed_channel_capacity": 32,
            "opportunity_channel_capacity": 8,
            "filter_name": "test-paper"
        },
        "pools": [
            {
                "kind": "pump",
                "pool": key(1),
                "fee_config": key(2),
                "pump_global": key(3),
                "pump_amm_global_config": key(4),
                "base_vault": key(5),
                "quote_vault": key(6),
                "base_mint": key(7),
                "quote_mint": key(8)
            },
            {
                "kind": "raydium_cpmm",
                "pool": key(9),
                "amm_config": key(10),
                "vault_0": key(11),
                "vault_1": key(12),
                "mint_0": key(13),
                "mint_1": key(14)
            }
        ],
        "cycles": [
            {
                "start_token": 0,
                "edges": [
                    {
                        "pool": 0,
                        "direction": "b_to_a",
                        "from_token": 0,
                        "to_token": 1
                    },
                    {
                        "pool": 1,
                        "direction": "a_to_b",
                        "from_token": 1,
                        "to_token": 0
                    }
                ],
                "search": {
                    "initial_seed": 1000,
                    "minimum_probe": 100,
                    "max_size": 10000,
                    "minimum_effective_profit": 1,
                    "max_slot_skew": 1,
                    "expected_cu": 120000
                }
            }
        ]
    })
    .to_string()
}

#[test]
fn config_builds_pipeline_and_deduplicated_account_filter() {
    let config = PaperConfig::from_json_str(&config_json()).unwrap();
    let built = config.build().unwrap();

    assert_eq!(built.filter_name, "test-paper");
    assert_eq!(built.account_filters.len(), 14);
    assert_eq!(built.feed_channel_capacity, 32);
    assert_eq!(built.opportunity_channel_capacity, 8);
}

#[test]
fn rejects_non_closed_cycles_before_runtime() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["cycles"][0]["edges"][1]["to_token"] = json!(99);

    let config = PaperConfig::from_json_str(&value.to_string()).unwrap();
    assert!(matches!(
        config.build(),
        Err(PaperConfigError::CycleNotClosed { .. })
    ));
}

#[test]
fn rejects_invalid_pubkeys_before_connecting_to_yellowstone() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["pools"][0]["pool"] = json!("not-a-pubkey");

    let config = PaperConfig::from_json_str(&value.to_string()).unwrap();
    assert!(matches!(
        config.build(),
        Err(PaperConfigError::InvalidPubkey { .. })
    ));
}
