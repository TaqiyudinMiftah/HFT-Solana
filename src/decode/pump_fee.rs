use crate::{
    decode::{
        check_discriminator, read_pubkey, read_u128, read_u32, read_u64, DecodeError,
    },
    quote::pump::{PumpFeeConfig, PumpFeeTier, PumpFeesBps},
};

pub const FEE_CONFIG_DISCRIMINATOR: [u8; 8] =
    [143, 52, 146, 187, 219, 123, 76, 155];

/// Account lengths documented by the current PumpSwap SDK.
pub const FEE_CONFIG_SIZE_PRE_STABLE: usize = 2_512;
pub const FEE_CONFIG_SIZE_POST_STABLE: usize = 4_073;
pub const FEE_CONFIG_SIZE_POST_EXOTIC: usize = 4_097;

const MAX_TIERS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedPumpFeeConfig {
    pub bump: u8,
    pub admin: [u8; 32],
    pub config: PumpFeeConfig,
}

#[inline]
fn read_fees(data: &[u8], cursor: &mut usize) -> Result<PumpFeesBps, DecodeError> {
    let fees = PumpFeesBps {
        lp_fee_bps: read_u64(data, *cursor)?,
        protocol_fee_bps: read_u64(data, *cursor + 8)?,
        creator_fee_bps: read_u64(data, *cursor + 16)?,
    };
    *cursor += 24;
    Ok(fees)
}

fn read_tiers(data: &[u8], cursor: &mut usize) -> Result<Vec<PumpFeeTier>, DecodeError> {
    let count = read_u32(data, *cursor)? as usize;
    *cursor += 4;

    if count > MAX_TIERS {
        return Err(DecodeError::InvalidValue);
    }

    let mut tiers = Vec::with_capacity(count);
    for _ in 0..count {
        let threshold = read_u128(data, *cursor)?;
        *cursor += 16;
        let fees = read_fees(data, cursor)?;
        tiers.push(PumpFeeTier {
            market_cap_threshold: threshold,
            fees,
        });
    }

    Ok(tiers)
}

/// Decode all currently known Pump FeeConfig layouts.
///
/// Older accounts have no stable tiers / exotic flat fees. Their account
/// length is used as the compatibility discriminator exactly as documented by
/// the current PumpSwap SDK.
pub fn decode_fee_config(data: &[u8]) -> Result<DecodedPumpFeeConfig, DecodeError> {
    if data.len() < 69 {
        return Err(DecodeError::TooShort);
    }
    check_discriminator(data, FEE_CONFIG_DISCRIMINATOR)?;

    let bump = data[8];
    let admin = read_pubkey(data, 9)?;

    let mut cursor = 41;
    let flat_fees = read_fees(data, &mut cursor)?;
    let fee_tiers = read_tiers(data, &mut cursor)?;

    let stable_fee_tiers = if data.len() >= FEE_CONFIG_SIZE_POST_STABLE {
        read_tiers(data, &mut cursor)?
    } else {
        Vec::new()
    };

    let exotic_flat_fees = if data.len() >= FEE_CONFIG_SIZE_POST_EXOTIC {
        read_fees(data, &mut cursor)?
    } else {
        PumpFeesBps::default()
    };

    Ok(DecodedPumpFeeConfig {
        bump,
        admin,
        config: PumpFeeConfig {
            flat_fees,
            fee_tiers,
            stable_fee_tiers,
            exotic_flat_fees,
        },
    })
}
