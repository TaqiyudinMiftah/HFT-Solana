use std::{collections::BTreeMap, str::FromStr};

use serde_json::json;
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
    rpc_request::RpcRequest,
    rpc_response::{OptionalContext, RpcKeyedAccount},
};
use solana_sdk_v2::{account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey};
use thiserror::Error;

use crate::{
    decode::meteora_dlmm::{BIN_ARRAY_DISCRIMINATOR, BIN_ARRAY_LB_PAIR_OFFSET, DLMM_PROGRAM_ID},
    feed::{AccountUpdate, FeedEvent},
};

const GET_MULTIPLE_ACCOUNTS_LIMIT: usize = 100;

#[derive(Clone, Debug)]
pub struct BootstrapRpcConfig {
    pub rpc_url: String,
    pub explicit_accounts: Vec<String>,
    pub dlmm_bin_pairs: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BootstrapSnapshot {
    pub events: Vec<FeedEvent>,
    pub explicit_accounts: usize,
    pub scoped_dlmm_bin_arrays: usize,
    pub max_context_slot: u64,
}

#[derive(Debug, Error)]
pub enum BootstrapRpcError {
    #[error("invalid bootstrap pubkey {0}")]
    InvalidPubkey(String),
    #[error("required bootstrap account is missing: {0}")]
    MissingAccount(String),
    #[error("bootstrap RPC request failed: {0}")]
    Rpc(String),
    #[error("bootstrap getProgramAccounts response did not include context")]
    MissingContext,
    #[error("bootstrap RPC account could not be decoded: {0}")]
    AccountDecode(String),
}

pub async fn fetch_bootstrap_snapshot(
    config: &BootstrapRpcConfig,
) -> Result<BootstrapSnapshot, BootstrapRpcError> {
    let rpc = RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::processed());

    let explicit_pubkeys = config
        .explicit_accounts
        .iter()
        .map(|value| {
            Pubkey::from_str(value).map_err(|_| BootstrapRpcError::InvalidPubkey(value.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut updates = BTreeMap::<[u8; 32], AccountUpdate>::new();
    let mut max_context_slot = 0u64;

    for chunk in explicit_pubkeys.chunks(GET_MULTIPLE_ACCOUNTS_LIMIT) {
        let response = rpc
            .get_multiple_accounts_with_config(
                chunk,
                RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    commitment: Some(CommitmentConfig::processed()),
                    data_slice: None,
                    min_context_slot: None,
                },
            )
            .await
            .map_err(|error| BootstrapRpcError::Rpc(error.to_string()))?;

        max_context_slot = max_context_slot.max(response.context.slot);

        for (pubkey, account) in chunk.iter().zip(response.value.into_iter()) {
            let account =
                account.ok_or_else(|| BootstrapRpcError::MissingAccount(pubkey.to_string()))?;
            updates.insert(
                pubkey.to_bytes(),
                startup_update(*pubkey, account, response.context.slot),
            );
        }
    }

    let explicit_accounts = updates.len();
    let dlmm_program = Pubkey::from_str(DLMM_PROGRAM_ID)
        .map_err(|_| BootstrapRpcError::InvalidPubkey(DLMM_PROGRAM_ID.to_owned()))?;
    let mut scoped_dlmm_bin_arrays = 0usize;

    for lb_pair in &config.dlmm_bin_pairs {
        let filters = vec![
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(0, BIN_ARRAY_DISCRIMINATOR.to_vec())),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                BIN_ARRAY_LB_PAIR_OFFSET as usize,
                lb_pair.to_vec(),
            )),
        ];

        let request_config = RpcProgramAccountsConfig {
            filters: Some(filters),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                commitment: Some(CommitmentConfig::processed()),
                data_slice: None,
                min_context_slot: (max_context_slot != 0).then_some(max_context_slot),
            },
            with_context: Some(true),
            ..RpcProgramAccountsConfig::default()
        };

        let response = rpc
            .send::<OptionalContext<Vec<RpcKeyedAccount>>>(
                RpcRequest::GetProgramAccounts,
                json!([dlmm_program.to_string(), request_config]),
            )
            .await
            .map_err(|error| BootstrapRpcError::Rpc(error.to_string()))?;

        let OptionalContext::Context(response) = response else {
            return Err(BootstrapRpcError::MissingContext);
        };
        let context_slot = response.context.slot;
        max_context_slot = max_context_slot.max(context_slot);

        for keyed in response.value {
            let pubkey = Pubkey::from_str(&keyed.pubkey)
                .map_err(|_| BootstrapRpcError::InvalidPubkey(keyed.pubkey.clone()))?;
            let account = keyed
                .account
                .decode()
                .ok_or_else(|| BootstrapRpcError::AccountDecode(pubkey.to_string()))?;
            let key = pubkey.to_bytes();
            if !updates.contains_key(&key) {
                scoped_dlmm_bin_arrays = scoped_dlmm_bin_arrays.saturating_add(1);
            }
            updates.insert(key, startup_update(pubkey, account, context_slot));
        }
    }

    Ok(BootstrapSnapshot {
        events: updates.into_values().map(FeedEvent::Account).collect(),
        explicit_accounts,
        scoped_dlmm_bin_arrays,
        max_context_slot,
    })
}

pub fn startup_account_slots(snapshot: &BootstrapSnapshot) -> BTreeMap<[u8; 32], u64> {
    snapshot
        .events
        .iter()
        .filter_map(|event| match event {
            FeedEvent::Account(update) => Some((update.pubkey, update.slot)),
            _ => None,
        })
        .collect()
}

pub fn live_event_is_not_older_than_bootstrap(
    event: &FeedEvent,
    startup_slots: &BTreeMap<[u8; 32], u64>,
) -> bool {
    match event {
        FeedEvent::Account(update) => startup_slots
            .get(&update.pubkey)
            .map_or(true, |startup_slot| update.slot >= *startup_slot),
        FeedEvent::DiscardBanks { .. } | FeedEvent::SlotComplete { .. } => true,
    }
}

fn startup_update(pubkey: Pubkey, account: Account, slot: u64) -> AccountUpdate {
    AccountUpdate {
        pubkey: pubkey.to_bytes(),
        owner: account.owner.to_bytes(),
        slot,
        write_version: 0,
        generation: 0,
        bank_id: None,
        is_startup: true,
        data: account.data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_filter_rejects_only_strictly_older_account_updates() {
        let snapshot = BootstrapSnapshot {
            events: vec![FeedEvent::Account(AccountUpdate {
                pubkey: [1u8; 32],
                owner: [2u8; 32],
                slot: 100,
                write_version: 0,
                generation: 0,
                bank_id: None,
                is_startup: true,
                data: vec![1],
            })],
            ..BootstrapSnapshot::default()
        };
        let slots = startup_account_slots(&snapshot);

        let live = |slot| {
            FeedEvent::Account(AccountUpdate {
                pubkey: [1u8; 32],
                owner: [2u8; 32],
                slot,
                write_version: 1,
                generation: 1,
                bank_id: Some(9),
                is_startup: false,
                data: vec![2],
            })
        };

        assert!(!live_event_is_not_older_than_bootstrap(&live(99), &slots));
        assert!(live_event_is_not_older_than_bootstrap(&live(100), &slots));
        assert!(live_event_is_not_older_than_bootstrap(&live(101), &slots));
        assert!(live_event_is_not_older_than_bootstrap(
            &FeedEvent::SlotComplete {
                bank: crate::feed::BankIdentity {
                    generation: 1,
                    slot: 99,
                    bank_id: 9,
                },
            },
            &slots,
        ));
    }

    #[test]
    fn startup_update_preserves_account_identity_and_payload() {
        let pubkey = Pubkey::new_from_array([1u8; 32]);
        let owner = Pubkey::new_from_array([2u8; 32]);
        let update = startup_update(
            pubkey,
            Account {
                lamports: 42,
                data: vec![7, 8, 9],
                owner,
                executable: false,
                rent_epoch: 3,
            },
            123,
        );

        assert_eq!(update.pubkey, [1u8; 32]);
        assert_eq!(update.owner, [2u8; 32]);
        assert_eq!(update.slot, 123);
        assert_eq!(update.write_version, 0);
        assert_eq!(update.generation, 0);
        assert_eq!(update.bank_id, None);
        assert!(update.is_startup);
        assert_eq!(update.data, vec![7, 8, 9]);
    }
}
