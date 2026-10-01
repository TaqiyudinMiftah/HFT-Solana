use crate::decode::{check_discriminator, read_pubkey, read_u64, DecodeError};

pub const PUMP_GLOBAL_DISCRIMINATOR: [u8; 8] =
    [167, 232, 232, 177, 200, 108, 114, 127];
pub const PUMP_AMM_GLOBAL_CONFIG_DISCRIMINATOR: [u8; 8] =
    [149, 8, 156, 202, 160, 252, 176, 217];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PumpGlobalAccount {
    pub whitelisted_quote_mints: [[u8; 32]; 1],
    pub creator_fee_configurable: bool,
    pub max_configurable_creator_fee_bps: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PumpAmmGlobalConfigAccount {
    pub creator_fee_configurable: bool,
    pub max_configurable_creator_fee_bps: u64,
}

pub fn decode_pump_global(data: &[u8]) -> Result<PumpGlobalAccount, DecodeError> {
    if data.len() < 1_054 {
        return Err(DecodeError::TooShort);
    }
    check_discriminator(data, PUMP_GLOBAL_DISCRIMINATOR)?;

    Ok(PumpGlobalAccount {
        whitelisted_quote_mints: [read_pubkey(data, 1_013)?],
        creator_fee_configurable: data[1_045] != 0,
        max_configurable_creator_fee_bps: read_u64(data, 1_046)?,
    })
}

pub fn decode_pump_amm_global_config(
    data: &[u8],
) -> Result<PumpAmmGlobalConfigAccount, DecodeError> {
    if data.len() < 949 {
        return Err(DecodeError::TooShort);
    }
    check_discriminator(data, PUMP_AMM_GLOBAL_CONFIG_DISCRIMINATOR)?;

    Ok(PumpAmmGlobalConfigAccount {
        creator_fee_configurable: data[940] != 0,
        max_configurable_creator_fee_bps: read_u64(data, 941)?,
    })
}
