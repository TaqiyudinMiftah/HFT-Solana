use hft_solana::{
    decode::pump_fee::{
        decode_fee_config, FEE_CONFIG_DISCRIMINATOR, FEE_CONFIG_SIZE_POST_EXOTIC,
        FEE_CONFIG_SIZE_PRE_STABLE,
    },
    quote::pump::{resolve_pool_fees, PumpFeeSchedule, PumpFeesBps},
};

fn put_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn put_u128(data: &mut [u8], offset: usize, value: u128) {
    data[offset..offset + 16].copy_from_slice(&value.to_le_bytes());
}

fn put_fees(data: &mut [u8], cursor: &mut usize, lp: u64, protocol: u64, creator: u64) {
    put_u64(data, *cursor, lp);
    put_u64(data, *cursor + 8, protocol);
    put_u64(data, *cursor + 16, creator);
    *cursor += 24;
}

fn fixture(len: usize, include_stable: bool, include_exotic: bool) -> Vec<u8> {
    let mut data = vec![0u8; len];
    data[..8].copy_from_slice(&FEE_CONFIG_DISCRIMINATOR);
    data[8] = 9;

    let mut cursor = 41;
    put_fees(&mut data, &mut cursor, 10, 20, 30);

    put_u32(&mut data, cursor, 2);
    cursor += 4;

    put_u128(&mut data, cursor, 100);
    cursor += 16;
    put_fees(&mut data, &mut cursor, 1, 2, 3);

    put_u128(&mut data, cursor, 200);
    cursor += 16;
    put_fees(&mut data, &mut cursor, 4, 5, 6);

    if include_stable {
        put_u32(&mut data, cursor, 1);
        cursor += 4;
        put_u128(&mut data, cursor, 50);
        cursor += 16;
        put_fees(&mut data, &mut cursor, 7, 8, 9);
    }

    if include_exotic {
        put_fees(&mut data, &mut cursor, 11, 12, 13);
    }

    data
}

#[test]
fn decodes_current_fee_config_and_resolves_all_schedules() {
    let decoded = decode_fee_config(&fixture(FEE_CONFIG_SIZE_POST_EXOTIC, true, true)).unwrap();

    assert_eq!(
        decoded.config.flat_fees,
        PumpFeesBps {
            lp_fee_bps: 10,
            protocol_fee_bps: 20,
            creator_fee_bps: 30,
        }
    );

    let sol = resolve_pool_fees(
        &decoded.config,
        PumpFeeSchedule::CanonicalSolLike,
        250,
        true,
        false,
        0,
        false,
    )
    .unwrap();
    assert_eq!(sol.creator_fee_bps, 6);

    let stable = resolve_pool_fees(
        &decoded.config,
        PumpFeeSchedule::CanonicalStable,
        999,
        true,
        false,
        0,
        false,
    )
    .unwrap();
    assert_eq!(stable.creator_fee_bps, 9);

    let exotic = resolve_pool_fees(
        &decoded.config,
        PumpFeeSchedule::CanonicalExotic,
        999,
        true,
        false,
        0,
        false,
    )
    .unwrap();
    assert_eq!(exotic.creator_fee_bps, 13);

    let noncanonical = resolve_pool_fees(
        &decoded.config,
        PumpFeeSchedule::NonCanonical,
        999,
        true,
        false,
        0,
        false,
    )
    .unwrap();
    assert_eq!(noncanonical.creator_fee_bps, 30);
}

#[test]
fn historical_config_falls_back_for_stable_and_exotic() {
    let decoded = decode_fee_config(&fixture(FEE_CONFIG_SIZE_PRE_STABLE, false, false)).unwrap();

    let stable = resolve_pool_fees(
        &decoded.config,
        PumpFeeSchedule::CanonicalStable,
        250,
        true,
        false,
        0,
        false,
    )
    .unwrap();
    assert_eq!(stable.creator_fee_bps, 6);

    let exotic = resolve_pool_fees(
        &decoded.config,
        PumpFeeSchedule::CanonicalExotic,
        250,
        true,
        false,
        0,
        false,
    )
    .unwrap();
    assert_eq!(exotic.creator_fee_bps, 30);
}

#[test]
fn creator_override_changes_only_creator_rate() {
    let decoded = decode_fee_config(&fixture(FEE_CONFIG_SIZE_POST_EXOTIC, true, true)).unwrap();

    let fees = resolve_pool_fees(
        &decoded.config,
        PumpFeeSchedule::CanonicalSolLike,
        250,
        true,
        true,
        99,
        false,
    )
    .unwrap();

    assert_eq!(fees.lp_fee_bps, 4);
    assert_eq!(fees.protocol_fee_bps, 5);
    assert_eq!(fees.creator_fee_bps, 99);

    let no_creator = resolve_pool_fees(
        &decoded.config,
        PumpFeeSchedule::CanonicalSolLike,
        250,
        false,
        true,
        99,
        false,
    )
    .unwrap();
    assert_eq!(no_creator.creator_fee_bps, 0);
}
