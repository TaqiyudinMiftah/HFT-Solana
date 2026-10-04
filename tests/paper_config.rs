#![cfg(feature = "yellowstone")]

use hft_solana::{
    landing::LandingProvider,
    paper_config::{PaperConfig, PaperConfigError},
};
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
        "tokens": [key(8), key(7)],
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
                "mint_0": key(7),
                "mint_1": key(8)
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
    assert_eq!(built.account_filters.len(), 12);
    assert_eq!(built.feed_channel_capacity, 32);
    assert_eq!(built.opportunity_channel_capacity, 8);
    assert!(built.landing.is_none());
}

#[test]
fn rejects_non_closed_cycles_before_runtime() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["tokens"].as_array_mut().unwrap().push(json!(key(15)));
    value["pools"][1]["mint_1"] = json!(key(15));
    value["cycles"][0]["edges"][1]["to_token"] = json!(2);

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

#[test]
fn rejects_cycle_direction_that_does_not_match_pool_mints() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["cycles"][0]["edges"][1]["direction"] = json!("b_to_a");

    let config = PaperConfig::from_json_str(&value.to_string()).unwrap();
    assert!(matches!(
        config.build(),
        Err(PaperConfigError::PoolMintMismatch { .. })
    ));
}

#[test]
fn dlmm_config_subscribes_pair_bins_bitmap_and_mints() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["pools"].as_array_mut().unwrap().push(json!({
        "kind": "meteora_dlmm",
        "lb_pair": key(20),
        "bin_arrays": [key(21), key(22)],
        "bitmap_extension": key(23),
        "mint_x": key(24),
        "mint_y": key(25)
    }));

    let config = PaperConfig::from_json_str(&value.to_string()).unwrap();
    let built = config.build().unwrap();

    for account in [key(20), key(21), key(22), key(23), key(24), key(25)] {
        assert!(built.account_filters.contains(&account));
    }
    assert_eq!(built.account_filters.len(), 18);
    assert_eq!(
        built.dlmm_bin_pairs,
        vec![Pubkey::new_from_array([20u8; 32]).to_bytes()]
    );
}

#[test]
fn dlmm_config_rejects_empty_bin_array_window() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["pools"].as_array_mut().unwrap().push(json!({
        "kind": "meteora_dlmm",
        "lb_pair": key(20),
        "bin_arrays": [],
        "bitmap_extension": null,
        "mint_x": key(24),
        "mint_y": key(25)
    }));

    let config = PaperConfig::from_json_str(&value.to_string()).unwrap();
    assert!(matches!(
        config.build(),
        Err(PaperConfigError::DlmmNoBinArrays)
    ));
}

#[test]
fn dlmm_config_rejects_zero_bin_array_take_count() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["pools"].as_array_mut().unwrap().push(json!({
        "kind": "meteora_dlmm",
        "lb_pair": key(20),
        "bin_arrays": [key(21), key(22)],
        "bitmap_extension": null,
        "mint_x": key(24),
        "mint_y": key(25),
        "bin_array_take_count": 0
    }));

    let config = PaperConfig::from_json_str(&value.to_string()).unwrap();
    assert!(matches!(
        config.build(),
        Err(PaperConfigError::DlmmZeroBinArrayTakeCount)
    ));
}

#[test]
fn landing_policy_builds_provider_candidates() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["landing"] = json!({
        "base_fee": 5000,
        "minimum_net_if_landed": 10000,
        "minimum_expected_value": 1000,
        "max_tip_share_bps": 6000,
        "candidates": [
            {
                "provider": "direct",
                "success_probability_bps": 6000,
                "priority_fee": 10000,
                "relay_tip": 0,
                "failure_fee": 15000
            },
            {
                "provider": "jito",
                "success_probability_bps": 9000,
                "priority_fee": 5000,
                "relay_tip": 20000,
                "failure_fee": 10000
            },
            {
                "provider": "helius_sender",
                "success_probability_bps": 8500,
                "priority_fee": 5000,
                "relay_tip": 15000,
                "failure_fee": 10000
            }
        ]
    });

    let built = PaperConfig::from_json_str(&value.to_string())
        .unwrap()
        .build()
        .unwrap();
    let landing = built.landing.unwrap();

    assert_eq!(landing.config.base_fee, 5000);
    assert_eq!(landing.config.max_tip_share_bps, 6000);
    assert_eq!(landing.candidates.len(), 3);
    assert_eq!(landing.candidates[0].provider, LandingProvider::Direct);
    assert_eq!(landing.candidates[1].provider, LandingProvider::Jito);
    assert_eq!(
        landing.candidates[2].provider,
        LandingProvider::HeliusSender
    );
}

#[test]
fn landing_policy_rejects_invalid_bps_inputs() {
    let mut value: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    value["landing"] = json!({
        "base_fee": 5000,
        "minimum_net_if_landed": 0,
        "minimum_expected_value": 0,
        "max_tip_share_bps": 10001,
        "candidates": [{
            "provider": "direct",
            "success_probability_bps": 10000,
            "priority_fee": 0,
            "relay_tip": 0,
            "failure_fee": 0
        }]
    });

    let config = PaperConfig::from_json_str(&value.to_string()).unwrap();
    assert!(matches!(
        config.build(),
        Err(PaperConfigError::LandingTipShareOutOfRange { .. })
    ));

    value["landing"]["max_tip_share_bps"] = json!(10000);
    value["landing"]["candidates"][0]["success_probability_bps"] = json!(10001);

    let config = PaperConfig::from_json_str(&value.to_string()).unwrap();
    assert!(matches!(
        config.build(),
        Err(PaperConfigError::LandingProbabilityOutOfRange { .. })
    ));
}
