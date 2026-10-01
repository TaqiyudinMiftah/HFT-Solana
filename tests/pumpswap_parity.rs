use hft_solana::{
    quote::pump::{buy_exact_quote_in, sell_exact_base_in, PumpFeesBps},
    types::StateVersion,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    kind: String,
    amount_in: u64,
    base_reserve: u64,
    raw_quote_reserve: u64,
    virtual_quote_reserves: i128,
    lp_fee_bps: u64,
    protocol_fee_bps: u64,
    creator_fee_bps: u64,
    cashback_coin: bool,
    expected_amount_out: u64,
    expected_effective_quote: u64,
    expected_lp_fee: u64,
    expected_protocol_fee: u64,
    expected_creator_fee: u64,
}

fn version() -> StateVersion {
    StateVersion {
        slot: 1,
        write_version: 1,
        generation: 1,
    }
}

#[test]
fn known_pumpswap_swaps_match_raw_units() {
    let fixtures: Vec<Fixture> =
        serde_json::from_str(include_str!("../fixtures/pumpswap_known_swaps.json")).unwrap();

    for f in fixtures {
        let fees = PumpFeesBps {
            lp_fee_bps: f.lp_fee_bps,
            protocol_fee_bps: f.protocol_fee_bps,
            creator_fee_bps: f.creator_fee_bps,
        };

        let q = match f.kind.as_str() {
            "buy" => buy_exact_quote_in(
                f.amount_in,
                f.base_reserve,
                f.raw_quote_reserve,
                f.virtual_quote_reserves,
                fees,
                f.cashback_coin,
                version(),
            )
            .unwrap(),
            "sell" => sell_exact_base_in(
                f.amount_in,
                f.base_reserve,
                f.raw_quote_reserve,
                f.virtual_quote_reserves,
                fees,
                f.cashback_coin,
                version(),
            )
            .unwrap(),
            other => panic!("unknown fixture kind: {other}"),
        };

        assert_eq!(q.quote.amount_out, f.expected_amount_out, "{:?}", f);
        assert_eq!(
            q.effective_quote_amount, f.expected_effective_quote,
            "{:?}",
            f
        );
        assert_eq!(q.lp_fee, f.expected_lp_fee, "{:?}", f);
        assert_eq!(q.protocol_fee, f.expected_protocol_fee, "{:?}", f);
        assert_eq!(q.creator_fee, f.expected_creator_fee, "{:?}", f);
        assert_eq!(q.quote.cashback, f.expected_creator_fee, "{:?}", f);
    }
}
