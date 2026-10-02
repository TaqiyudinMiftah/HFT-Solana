pub mod meteora_damm;
pub mod mint;
pub mod pump;
pub mod pump_fee;
pub mod pump_global;
pub mod raydium;
pub mod token;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("account data is too short")]
    TooShort,
    #[error("anchor discriminator mismatch")]
    BadDiscriminator,
    #[error("invalid enum or flag value")]
    InvalidValue,
    #[error("integer overflow or underflow")]
    Math,
}

#[inline]
pub(crate) fn read_u16(data: &[u8], offset: usize) -> Result<u16, DecodeError> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .ok_or(DecodeError::TooShort)?
        .try_into()
        .map_err(|_| DecodeError::TooShort)?;
    Ok(u16::from_le_bytes(bytes))
}

#[inline]
pub(crate) fn read_u32(data: &[u8], offset: usize) -> Result<u32, DecodeError> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .ok_or(DecodeError::TooShort)?
        .try_into()
        .map_err(|_| DecodeError::TooShort)?;
    Ok(u32::from_le_bytes(bytes))
}

#[inline]
pub(crate) fn read_u64(data: &[u8], offset: usize) -> Result<u64, DecodeError> {
    let bytes: [u8; 8] = data
        .get(offset..offset + 8)
        .ok_or(DecodeError::TooShort)?
        .try_into()
        .map_err(|_| DecodeError::TooShort)?;
    Ok(u64::from_le_bytes(bytes))
}

#[inline]
pub(crate) fn read_u128(data: &[u8], offset: usize) -> Result<u128, DecodeError> {
    let bytes: [u8; 16] = data
        .get(offset..offset + 16)
        .ok_or(DecodeError::TooShort)?
        .try_into()
        .map_err(|_| DecodeError::TooShort)?;
    Ok(u128::from_le_bytes(bytes))
}

#[inline]
pub(crate) fn read_i128(data: &[u8], offset: usize) -> Result<i128, DecodeError> {
    let bytes: [u8; 16] = data
        .get(offset..offset + 16)
        .ok_or(DecodeError::TooShort)?
        .try_into()
        .map_err(|_| DecodeError::TooShort)?;
    Ok(i128::from_le_bytes(bytes))
}

#[inline]
pub(crate) fn read_pubkey(data: &[u8], offset: usize) -> Result<[u8; 32], DecodeError> {
    data.get(offset..offset + 32)
        .ok_or(DecodeError::TooShort)?
        .try_into()
        .map_err(|_| DecodeError::TooShort)
}

#[inline]
pub(crate) fn check_discriminator(data: &[u8], discriminator: [u8; 8]) -> Result<(), DecodeError> {
    if data.get(..8) != Some(discriminator.as_slice()) {
        return Err(DecodeError::BadDiscriminator);
    }
    Ok(())
}
