use hft_solana::{
    quote::cpmm::{fee_floor, quote_xyk_exact_in},
    types::StateVersion,
};

fn version() -> StateVersion {
    StateVersion {
        slot: 1,
        write_version: 1,
        generation: 1,
    }
}

#[test]
fn fee_uses_integer_floor() {
    assert_eq!(fee_floor(1_000_000, 3_000).unwrap(), 3_000);
}

#[test]
fn cpmm_output_is_monotonic_for_larger_input() {
    let small = quote_xyk_exact_in(
        10_000,
        10_000_000,
        20_000_000,
        3_000,
        version(),
    )
    .unwrap();

    let large = quote_xyk_exact_in(
        20_000,
        10_000_000,
        20_000_000,
        3_000,
        version(),
    )
    .unwrap();

    assert!(large.amount_out > small.amount_out);
    assert!(large.dex_fee >= small.dex_fee);
}

#[test]
fn cpmm_rejects_zero_input() {
    assert!(quote_xyk_exact_in(
        0,
        10_000_000,
        20_000_000,
        3_000,
        version(),
    )
    .is_err());
}
