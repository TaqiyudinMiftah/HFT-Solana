use hft_solana::{
    decode::meteora_damm::{MeteoraDynamicFee, MeteoraPoolFees},
    quote::meteora_damm::{
        excluded_fee_amount, fee_mode, fee_on_amount, included_fee_amount, max_fee_numerator,
        total_fee_numerator, variable_fee_numerator, MeteoraSplitFees,
    },
    types::Direction,
};

fn dynamic(initialized: bool) -> MeteoraDynamicFee {
    MeteoraDynamicFee {
        initialized,
        max_volatility_accumulator: 0,
        variable_fee_control: 1_000,
        bin_step: 10,
        filter_period: 0,
        decay_period: 0,
        reduction_factor: 0,
        last_update_timestamp: 0,
        bin_step_u128: 0,
        sqrt_price_reference: 0,
        volatility_accumulator: 1_000,
        volatility_reference: 0,
    }
}

fn fees() -> MeteoraPoolFees {
    MeteoraPoolFees {
        base_fee_info: [0u8; 32],
        protocol_fee_percent: 20,
        referral_fee_percent: 25,
        compounding_fee_bps: 4_000,
        dynamic_fee: dynamic(false),
        init_sqrt_price: 0,
    }
}

#[test]
fn dynamic_fee_matches_current_integer_formula() {
    // ((1000 * 10)^2 * 1000 + 1e11 - 1) / 1e11 = 1
    assert_eq!(variable_fee_numerator(&dynamic(true)).unwrap(), 1);
    assert_eq!(variable_fee_numerator(&dynamic(false)).unwrap(), 0);
}

#[test]
fn total_fee_is_capped_by_pool_fee_version() {
    let mut d = dynamic(true);
    d.volatility_accumulator = 1_000_000;
    d.bin_step = 100;
    d.variable_fee_control = 1_000_000;

    assert_eq!(max_fee_numerator(0).unwrap(), 500_000_000);
    assert_eq!(max_fee_numerator(1).unwrap(), 990_000_000);
    assert_eq!(total_fee_numerator(490_000_000, &d, 0).unwrap(), 500_000_000);
    assert_eq!(total_fee_numerator(980_000_000, &d, 1).unwrap(), 990_000_000);
    assert!(max_fee_numerator(2).is_err());
}

#[test]
fn included_and_excluded_fee_rounding_matches_program_direction() {
    let (net, fee) = excluded_fee_amount(100_000_000, 1_000_000).unwrap();
    assert_eq!((net, fee), (900_000, 100_000));

    let (gross, fee) = included_fee_amount(100_000_000, 900_000).unwrap();
    assert_eq!((gross, fee), (1_000_000, 100_000));

    // Ceiling behavior on a non-divisible input.
    let (net, fee) = excluded_fee_amount(1, 1).unwrap();
    assert_eq!((net, fee), (0, 1));
}

#[test]
fn splits_protocol_referral_claiming_and_compounding_fees() {
    let result = fee_on_amount(&fees(), 1_000_000, 100_000_000, true).unwrap();

    assert_eq!(result.amount, 900_000);
    assert_eq!(result.trading_fee, 100_000);
    assert_eq!(
        result.split,
        MeteoraSplitFees {
            claiming_fee: 48_000,
            compounding_fee: 32_000,
            protocol_fee: 15_000,
            referral_fee: 5_000,
        }
    );
}

#[test]
fn fee_mode_matches_collect_fee_mode_and_trade_direction() {
    let both_a_to_b = fee_mode(0, Direction::AtoB, false).unwrap();
    assert!(!both_a_to_b.fees_on_input);
    assert!(!both_a_to_b.fees_on_token_a);

    let both_b_to_a = fee_mode(0, Direction::BtoA, false).unwrap();
    assert!(!both_b_to_a.fees_on_input);
    assert!(both_b_to_a.fees_on_token_a);

    let only_b_b_to_a = fee_mode(1, Direction::BtoA, true).unwrap();
    assert!(only_b_b_to_a.fees_on_input);
    assert!(!only_b_b_to_a.fees_on_token_a);
    assert!(only_b_b_to_a.has_referral);

    let compounding_b_to_a = fee_mode(2, Direction::BtoA, false).unwrap();
    assert!(compounding_b_to_a.fees_on_input);

    assert!(fee_mode(3, Direction::AtoB, false).is_err());
}
