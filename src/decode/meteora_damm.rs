use crate::decode::{
    check_discriminator, read_pubkey, read_u16, read_u32, read_u64, read_u128, DecodeError,
};

/// Meteora DAMM v2 / cp-amm program:
/// cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG
///
/// The Pool account discriminator intentionally collides with PumpSwap's
/// Pool discriminator. Callers must route accounts by owner/program, never
/// by discriminator alone.
pub const POOL_DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

/// Official program source asserts Pool::INIT_SPACE == 1104. Anchor's account
/// discriminator adds 8 bytes on-chain.
pub const POOL_ACCOUNT_LEN: usize = 8 + 1_104;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeteoraDynamicFee {
    pub initialized: bool,
    pub max_volatility_accumulator: u32,
    pub variable_fee_control: u32,
    pub bin_step: u16,
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub last_update_timestamp: u64,
    pub bin_step_u128: u128,
    pub sqrt_price_reference: u128,
    pub volatility_accumulator: u128,
    pub volatility_reference: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeteoraPoolFees {
    /// Opaque BaseFeeInfo bytes. Exact quoting must dispatch through the
    /// official base-fee mode handlers instead of assuming one formula.
    pub base_fee_info: [u8; 32],
    pub protocol_fee_percent: u8,
    pub referral_fee_percent: u8,
    pub compounding_fee_bps: u16,
    pub dynamic_fee: MeteoraDynamicFee,
    pub init_sqrt_price: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeteoraDammPoolAccount {
    pub fees: MeteoraPoolFees,
    pub token_a_mint: [u8; 32],
    pub token_b_mint: [u8; 32],
    pub token_a_vault: [u8; 32],
    pub token_b_vault: [u8; 32],
    pub whitelisted_vault: [u8; 32],
    pub liquidity: u128,
    pub protocol_a_fee: u64,
    pub protocol_b_fee: u64,
    pub dead_liquidity_fee_checkpoint: u64,
    pub sqrt_min_price: u128,
    pub sqrt_max_price: u128,
    pub sqrt_price: u128,
    pub activation_point: u64,
    pub activation_type: u8,
    pub pool_status: u8,
    pub token_a_flag: u8,
    pub token_b_flag: u8,
    pub collect_fee_mode: u8,
    pub pool_type: u8,
    pub fee_version: u8,
    pub permanent_lock_liquidity: u128,
    pub creator: [u8; 32],
    pub token_a_amount: u64,
    pub token_b_amount: u64,
    pub layout_version: u8,
}

/// Decode the current official zero-copy Meteora DAMM v2 Pool layout.
///
/// Offsets follow the program's account(zero_copy) repr(C) structure.
/// The source contains explicit padding and compile-time size assertions:
/// PoolFeesStruct=160, PoolMetrics=80, RewardInfo=192, Pool=1104.
pub fn decode_pool(data: &[u8]) -> Result<MeteoraDammPoolAccount, DecodeError> {
    if data.len() < POOL_ACCOUNT_LEN {
        return Err(DecodeError::TooShort);
    }
    check_discriminator(data, POOL_DISCRIMINATOR)?;

    let base_fee_info: [u8; 32] = data
        .get(8..40)
        .ok_or(DecodeError::TooShort)?
        .try_into()
        .map_err(|_| DecodeError::TooShort)?;

    let dynamic_fee = MeteoraDynamicFee {
        initialized: data[56] != 0,
        max_volatility_accumulator: read_u32(data, 64)?,
        variable_fee_control: read_u32(data, 68)?,
        bin_step: read_u16(data, 72)?,
        filter_period: read_u16(data, 74)?,
        decay_period: read_u16(data, 76)?,
        reduction_factor: read_u16(data, 78)?,
        last_update_timestamp: read_u64(data, 80)?,
        bin_step_u128: read_u128(data, 88)?,
        sqrt_price_reference: read_u128(data, 104)?,
        volatility_accumulator: read_u128(data, 120)?,
        volatility_reference: read_u128(data, 136)?,
    };

    Ok(MeteoraDammPoolAccount {
        fees: MeteoraPoolFees {
            base_fee_info,
            protocol_fee_percent: data[48],
            referral_fee_percent: data[50],
            compounding_fee_bps: read_u16(data, 54)?,
            dynamic_fee,
            init_sqrt_price: read_u128(data, 152)?,
        },
        token_a_mint: read_pubkey(data, 168)?,
        token_b_mint: read_pubkey(data, 200)?,
        token_a_vault: read_pubkey(data, 232)?,
        token_b_vault: read_pubkey(data, 264)?,
        whitelisted_vault: read_pubkey(data, 296)?,
        liquidity: read_u128(data, 360)?,
        protocol_a_fee: read_u64(data, 392)?,
        protocol_b_fee: read_u64(data, 400)?,
        dead_liquidity_fee_checkpoint: read_u64(data, 408)?,
        sqrt_min_price: read_u128(data, 424)?,
        sqrt_max_price: read_u128(data, 440)?,
        sqrt_price: read_u128(data, 456)?,
        activation_point: read_u64(data, 472)?,
        activation_type: data[480],
        pool_status: data[481],
        token_a_flag: data[482],
        token_b_flag: data[483],
        collect_fee_mode: data[484],
        pool_type: data[485],
        fee_version: data[486],
        permanent_lock_liquidity: read_u128(data, 552)?,
        creator: read_pubkey(data, 648)?,
        token_a_amount: read_u64(data, 680)?,
        token_b_amount: read_u64(data, 688)?,
        layout_version: data[696],
    })
}
