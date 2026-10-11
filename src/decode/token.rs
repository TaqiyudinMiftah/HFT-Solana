use crate::decode::{read_pubkey, read_u64, DecodeError};

pub const TOKEN_ACCOUNT_BASE_LEN: usize = 165;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenAccountState {
    Uninitialized,
    Initialized,
    Frozen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenAccountBase {
    pub mint: [u8; 32],
    pub owner: [u8; 32],
    pub amount: u64,
    pub state: TokenAccountState,
}

/// Decode the base SPL Token account layout.
///
/// Token-2022 keeps the same base account state and appends extension data,
/// so this function intentionally ignores bytes after the 165-byte base.
pub fn decode_token_account_base(data: &[u8]) -> Result<TokenAccountBase, DecodeError> {
    if data.len() < TOKEN_ACCOUNT_BASE_LEN {
        return Err(DecodeError::TooShort);
    }

    let state = match data[108] {
        0 => TokenAccountState::Uninitialized,
        1 => TokenAccountState::Initialized,
        2 => TokenAccountState::Frozen,
        _ => return Err(DecodeError::InvalidValue),
    };

    Ok(TokenAccountBase {
        mint: read_pubkey(data, 0)?,
        owner: read_pubkey(data, 32)?,
        amount: read_u64(data, 64)?,
        state,
    })
}
