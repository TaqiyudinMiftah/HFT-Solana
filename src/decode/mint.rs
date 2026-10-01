use spl_token_2022_interface::{
    extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions},
    state::Mint,
};

use crate::snapshot::MintQuoteSafety;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MintQuoteInfo {
    pub supply: u64,
    pub decimals: u8,
    pub safety: MintQuoteSafety,
    pub extension_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MintInspectError {
    InvalidMintData,
    InvalidExtensionData,
}

/// Conservative quote-safety classification.
///
/// Safe entries below do not alter raw transfer amount or require extra
/// transfer execution semantics. All unknown/future variants are rejected.
pub fn extension_is_quote_safe(extension: ExtensionType) -> bool {
    matches!(
        extension,
        ExtensionType::Uninitialized
            | ExtensionType::MintCloseAuthority
            | ExtensionType::DefaultAccountState
            | ExtensionType::InterestBearingConfig
            | ExtensionType::PermanentDelegate
            | ExtensionType::MetadataPointer
            | ExtensionType::TokenMetadata
            | ExtensionType::GroupPointer
            | ExtensionType::TokenGroup
            | ExtensionType::GroupMemberPointer
            | ExtensionType::TokenGroupMember
            | ExtensionType::ScaledUiAmount
            | ExtensionType::PermissionedBurn
    )
}

pub fn inspect_mint(data: &[u8]) -> Result<MintQuoteInfo, MintInspectError> {
    let state =
        StateWithExtensions::<Mint>::unpack(data).map_err(|_| MintInspectError::InvalidMintData)?;

    let extensions = state
        .get_extension_types()
        .map_err(|_| MintInspectError::InvalidExtensionData)?;

    let safety = if extensions.iter().copied().all(extension_is_quote_safe) {
        MintQuoteSafety::Safe
    } else {
        MintQuoteSafety::UnsupportedOrUnknown
    };

    Ok(MintQuoteInfo {
        supply: state.base.supply,
        decimals: state.base.decimals,
        safety,
        extension_count: extensions.len(),
    })
}
