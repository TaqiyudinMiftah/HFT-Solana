use hft_solana::{
    graph::{Cycle, Edge},
    quote::raydium::RaydiumFees,
    search::quote_cycle,
    sizing::{optimize_size, optimize_size_bounded},
    state::{CreatorFeeOn, PoolCell, PoolState, RaydiumCpmmState},
    types::{Direction, StateVersion},
};

fn version() -> StateVersion {
    StateVersion {
        slot: 100,
        write_version: 1,
        generation: 1,
    }
}

fn pool(reserve_a: u64, reserve_b: u64) -> PoolCell {
    PoolCell::new(PoolState::RaydiumCpmm(RaydiumCpmmState {
        version: version(),
        reserve_a,
        reserve_b,
        fees: RaydiumFees {
            trade_fee_rate: 0,
            creator_fee_rate: 0,
            protocol_fee_rate: 0,
            fund_fee_rate: 0,
        },
        creator_fee_on: CreatorFeeOn::Both,
    }))
}

fn cycle() -> Cycle {
    Cycle {
        id: 0,
        len: 2,
        start_token: 0,
        edges: [
            Edge {
                pool: 0,
                direction: Direction::AtoB,
                from_token: 0,
                to_token: 1,
            },
            Edge {
                pool: 1,
                direction: Direction::BtoA,
                from_token: 1,
                to_token: 0,
            },
            Edge {
                pool: 0,
                direction: Direction::AtoB,
                from_token: 0,
                to_token: 0,
            },
        ],
    }
}

#[test]
fn bracketed_optimizer_matches_bruteforce_peak_profit() {
    let pools = vec![pool(100_000, 100_000), pool(120_000, 100_000)];
    let cycle = cycle();

    let brute = (10..=20_000)
        .filter_map(|amount| quote_cycle(&cycle, &pools, amount).ok())
        .max_by_key(|result| result.effective_profit)
        .unwrap();

    // Deliberately start far below the optimum.
    let optimized = optimize_size_bounded(&cycle, &pools, 100, 10, 20_000).unwrap();

    assert_eq!(optimized.effective_profit, brute.effective_profit);
    assert!((10..=20_000).contains(&optimized.amount_in));
}

#[test]
fn bracketed_optimizer_improves_over_old_five_probe_window() {
    let pools = vec![pool(100_000, 100_000), pool(120_000, 100_000)];
    let cycle = cycle();

    let optimized = optimize_size_bounded(&cycle, &pools, 100, 10, 20_000).unwrap();

    let old_probe_best = [25, 50, 100, 200, 400]
        .into_iter()
        .filter_map(|amount| quote_cycle(&cycle, &pools, amount).ok())
        .max_by_key(|result| result.effective_profit)
        .unwrap();

    assert!(optimized.effective_profit > old_probe_best.effective_profit);
    assert!(optimized.amount_in > 400);
}

#[test]
fn compatibility_wrapper_still_returns_a_valid_quote() {
    let pools = vec![pool(100_000, 100_000), pool(120_000, 100_000)];
    let cycle = cycle();

    let result = optimize_size(&cycle, &pools, 100, 20_000).unwrap();
    assert!(result.amount_in >= 1);
    assert!(result.amount_in <= 20_000);
}
