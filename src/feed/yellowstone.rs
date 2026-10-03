use std::collections::HashMap;

use futures::StreamExt;
use thiserror::Error;
use tokio::sync::mpsc;
use yellowstone_grpc_client::{GeyserGrpcClient, ReconnectEvent};
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, CommitmentLevel, SlotStatus, SubscribeRequest,
    SubscribeRequestFilterAccounts, SubscribeRequestFilterSlots,
};

use super::{AccountUpdate, BankIdentity, FeedEvent};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct YellowstoneAccountFilter {
    /// Base58 account addresses to subscribe to.
    pub accounts: Vec<String>,
    /// Base58 owner program addresses to subscribe to.
    pub owners: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YellowstoneConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub filter_name: String,
    pub filter: YellowstoneAccountFilter,
}

#[derive(Debug, Error)]
pub enum YellowstoneFeedError {
    #[error("invalid Yellowstone endpoint/client configuration: {0}")]
    Client(String),
    #[error("failed to connect to Yellowstone: {0}")]
    Connect(String),
    #[error("failed to start Yellowstone subscription: {0}")]
    Subscribe(String),
    #[error("Yellowstone stream failed: {0}")]
    Stream(String),
    #[error("account update pubkey or owner is not 32 bytes")]
    InvalidPubkey,
    #[error("paper-feed receiver closed")]
    ReceiverClosed,
}

pub fn build_subscribe_request(config: &YellowstoneConfig) -> SubscribeRequest {
    let mut accounts = HashMap::new();
    accounts.insert(
        config.filter_name.clone(),
        SubscribeRequestFilterAccounts {
            account: config.filter.accounts.clone(),
            owner: config.filter.owners.clone(),
            ..Default::default()
        },
    );

    let mut slots = HashMap::new();
    slots.insert(
        "slot_fence".to_owned(),
        SubscribeRequestFilterSlots {
            filter_by_commitment: Some(false),
            interslot_updates: Some(true),
        },
    );

    SubscribeRequest {
        accounts,
        slots,
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    }
}

fn array32(bytes: &[u8]) -> Result<[u8; 32], YellowstoneFeedError> {
    bytes
        .try_into()
        .map_err(|_| YellowstoneFeedError::InvalidPubkey)
}

pub async fn run_account_feed(
    config: YellowstoneConfig,
    output: mpsc::Sender<FeedEvent>,
) -> Result<(), YellowstoneFeedError> {
    let request = build_subscribe_request(&config);

    let builder = GeyserGrpcClient::build_from_shared(config.endpoint)
        .map_err(|error| YellowstoneFeedError::Client(error.to_string()))?
        .x_token(config.x_token)
        .map_err(|error| YellowstoneFeedError::Client(error.to_string()))?;

    let mut client = builder
        .connect()
        .await
        .map_err(|error| YellowstoneFeedError::Connect(error.to_string()))?;

    let (_sink, mut stream) = client
        .subscribe_with_reconnect(Some(request))
        .await
        .map_err(|error| YellowstoneFeedError::Subscribe(error.to_string()))?;

    while let Some(message) = stream.next().await {
        match message {
            Ok(ReconnectEvent::Update { generation, update }) => {
                let event = match update.update_oneof {
                    Some(UpdateOneof::Account(account_update)) => {
                        let Some(account) = account_update.account else {
                            continue;
                        };

                        FeedEvent::Account(AccountUpdate {
                            pubkey: array32(&account.pubkey)?,
                            owner: array32(&account.owner)?,
                            slot: account_update.slot,
                            write_version: account.write_version,
                            generation,
                            bank_id: account_update.bank_id,
                            is_startup: account_update.is_startup,
                            data: account.data,
                        })
                    }
                    Some(UpdateOneof::Slot(slot_update)) => {
                        let Ok(status) = SlotStatus::try_from(slot_update.status) else {
                            continue;
                        };
                        if status != SlotStatus::SlotCompleted {
                            continue;
                        }
                        let Some(bank_id) = slot_update.bank_id else {
                            continue;
                        };

                        FeedEvent::SlotComplete {
                            bank: BankIdentity {
                                generation,
                                slot: slot_update.slot,
                                bank_id,
                            },
                        }
                    }
                    _ => continue,
                };

                output
                    .send(event)
                    .await
                    .map_err(|_| YellowstoneFeedError::ReceiverClosed)?;
            }
            Ok(ReconnectEvent::DiscardBanks { banks, reason, .. }) => {
                let event = FeedEvent::DiscardBanks {
                    banks: banks
                        .into_iter()
                        .map(|bank| BankIdentity {
                            generation: bank.generation,
                            slot: bank.slot,
                            bank_id: bank.bank_id,
                        })
                        .collect(),
                    reason: format!("{reason:?}"),
                };

                output
                    .send(event)
                    .await
                    .map_err(|_| YellowstoneFeedError::ReceiverClosed)?;
            }
            Err(status) => {
                return Err(YellowstoneFeedError::Stream(status.to_string()));
            }
        }
    }

    Ok(())
}
