use hft_solana::{
    quote::raydium::{quote_base_input, RaydiumFees},
    types::StateVersion,
};

fn version() -> StateVersion {
    StateVersion {
        slot: 1,
        write_version: 1,
        generation: 1,
    }
}

fn fees() -> RaydiumFees {
    RaydiumFees {
        trade_fee_rate: 2_500,
        creator_fee_rate: 1_000,
        protocol_fee_rate: 120_000,
        fund_fee_rate: 40_000,
    }
}

#[test]
fn creator_fee_on_input_matches_current_raydium_rounding() {
    let q = quote_base_input(
        1_000_000,
        10_000_000_000,
        20_000_000_000,
        fees(),
        true,
        version(),
    )
    .unwrap();

    assert_eq!(q.trade_fee, 2_500);
    assert_eq!(q.creator_fee, 1_000);
    assert_eq!(q.protocol_fee, 300);
    assert_eq!(q.fund_fee, 100);
    assert_eq!(q.curve_output, 1_992_801);
    assert_eq!(q.quote.amount_out, 1_992_801);
}

#[test]
fn creator_fee_on_output_matches_current_raydium_rounding() {
    let q = quote_base_input(
        1_000_000,
        10_000_000_000,
        20_000_000_000,
        fees(),
        false,
        version(),
    )
    .unwrap();

    assert_eq!(q.trade_fee, 2_500);
    assert_eq!(q.creator_fee, 1_995);
    assert_eq!(q.protocol_fee, 300);
    assert_eq!(q.fund_fee, 100);
    assert_eq!(q.curve_output, 1_994_801);
    assert_eq!(q.quote.amount_out, 1_992_806);
}
