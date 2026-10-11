use std::{collections::HashMap, time::Duration};

use futures::StreamExt;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use yellowstone_grpc_client::{
    ClientTlsConfig, GeyserGrpcClient, GeyserGrpcClientError, ReconnectEvent,
};
use yellowstone_grpc_proto::prelude::{
    subscribe_request_filter_accounts_filter::Filter as AccountFilterOneof,
    subscribe_request_filter_accounts_filter_memcmp::Data as MemcmpData,
    subscribe_update::UpdateOneof, CommitmentLevel, SlotStatus, SubscribeRequest,
    SubscribeRequestFilterAccounts, SubscribeRequestFilterAccountsFilter,
    SubscribeRequestFilterAccountsFilterMemcmp, SubscribeRequestFilterSlots, SubscribeUpdateSlot,
};
use yellowstone_grpc_proto::tonic::Code;

use super::{AccountUpdate, BankIdentity, FeedEvent};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YellowstoneMemcmpFilter {
    pub offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YellowstoneScopedAccountFilter {
    pub name: String,
    pub owners: Vec<String>,
    pub memcmp: Vec<YellowstoneMemcmpFilter>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct YellowstoneAccountFilter {
    /// Base58 account addresses to subscribe to.
    pub accounts: Vec<String>,
    /// Base58 owner program addresses to subscribe to.
    pub owners: Vec<String>,
    /// Additional immutable account scopes. Each scope becomes a separate
    /// Yellowstone account filter; memcmp predicates inside one scope are ANDed.
    pub scoped: Vec<YellowstoneScopedAccountFilter>,
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

/// Explain provider failures without retaining server messages or credential URLs.
pub fn provider_error_summary(error: &GeyserGrpcClientError) -> String {
    match error {
        GeyserGrpcClientError::TransportError(_) => {
            "transport/TLS failure; verify host, port, certificate roots, and network".to_owned()
        }
        GeyserGrpcClientError::TonicStatus(status) => {
            let action = match status.code() {
                Code::Unimplemented => "method unavailable; verify the Yellowstone gRPC endpoint and streaming access with the provider; use paper_bootstrap for RPC-only observations",
                Code::Unauthenticated => "authentication rejected; verify the provider's x-token locally",
                Code::PermissionDenied => "access denied; verify application streaming permissions with the provider",
                Code::ResourceExhausted => "provider quota exhausted; reduce subscriptions or request rate",
                Code::Unavailable | Code::DeadlineExceeded => "service unavailable or timed out; check provider status and network",
                Code::FailedPrecondition => "provider/client recovery requirements not satisfied; verify bank metadata and replay support",
                _ => "provider request failed; verify subscription filters and protocol compatibility",
            };
            format!(
                "gRPC {:?}: {action} (provider details omitted)",
                status.code()
            )
        }
    }
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

    for scope in &config.filter.scoped {
        let filters = scope
            .memcmp
            .iter()
            .map(|memcmp| SubscribeRequestFilterAccountsFilter {
                filter: Some(AccountFilterOneof::Memcmp(
                    SubscribeRequestFilterAccountsFilterMemcmp {
                        offset: memcmp.offset,
                        data: Some(MemcmpData::Bytes(memcmp.bytes.clone())),
                    },
                )),
            })
            .collect();

        accounts.insert(
            format!("{}:{}", config.filter_name, scope.name),
            SubscribeRequestFilterAccounts {
                owner: scope.owners.clone(),
                filters,
                ..Default::default()
            },
        );
    }

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

/// Translate a processed bank's account-delivery fence without inventing identity.
pub fn slot_fence_event(generation: u64, update: SubscribeUpdateSlot) -> Option<FeedEvent> {
    // Geyser emits PROCESSED after account notifications. COMPLETED refers
    // to received shreds and does not carry a bank id in Yellowstone's schema.
    if SlotStatus::try_from(update.status).ok()? != SlotStatus::SlotProcessed {
        return None;
    }
    Some(FeedEvent::SlotComplete {
        bank: BankIdentity {
            generation,
            slot: update.slot,
            bank_id: update.bank_id?,
        },
    })
}

pub async fn run_account_feed(
    config: YellowstoneConfig,
    output: mpsc::Sender<FeedEvent>,
) -> Result<(), YellowstoneFeedError> {
    run_account_feed_inner(config, output, None).await
}

pub async fn run_account_feed_with_ready(
    config: YellowstoneConfig,
    output: mpsc::Sender<FeedEvent>,
    ready: oneshot::Sender<()>,
) -> Result<(), YellowstoneFeedError> {
    run_account_feed_inner(config, output, Some(ready)).await
}

async fn run_account_feed_inner(
    config: YellowstoneConfig,
    output: mpsc::Sender<FeedEvent>,
    ready: Option<oneshot::Sender<()>>,
) -> Result<(), YellowstoneFeedError> {
    let request = build_subscribe_request(&config);
    let startup_timeout = Duration::from_secs(15);
    let https = config
        .endpoint
        .split_once(':')
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("https"));

    let mut builder = GeyserGrpcClient::build_from_shared(config.endpoint)
        .map_err(|_| YellowstoneFeedError::Client("invalid endpoint URI".to_owned()))?
        .x_token(config.x_token)
        .map_err(|_| YellowstoneFeedError::Client("invalid x-token metadata".to_owned()))?
        .connect_timeout(startup_timeout);
    if https {
        builder = builder
            .tls_config(ClientTlsConfig::new().with_native_roots())
            .map_err(|_| {
                YellowstoneFeedError::Client("TLS configuration failed; details omitted".to_owned())
            })?;
    }

    let mut client = tokio::time::timeout(startup_timeout, builder.connect())
        .await
        .map_err(|_| YellowstoneFeedError::Connect("connection timed out after 15s".to_owned()))?
        .map_err(|_| {
            YellowstoneFeedError::Connect(
                "transport/TLS failure; verify host, port, certificate roots, and network"
                    .to_owned(),
            )
        })?;

    let (_sink, mut stream) = tokio::time::timeout(
        startup_timeout,
        client.subscribe_with_reconnect(Some(request)),
    )
    .await
    .map_err(|_| YellowstoneFeedError::Subscribe("subscription timed out after 15s".to_owned()))?
    .map_err(|error| YellowstoneFeedError::Subscribe(provider_error_summary(&error)))?;

    if let Some(ready) = ready {
        let _ = ready.send(());
    }

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
                        let Some(event) = slot_fence_event(generation, slot_update) else {
                            continue;
                        };
                        event
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
                return Err(YellowstoneFeedError::Stream(provider_error_summary(
                    &GeyserGrpcClientError::TonicStatus(status),
                )));
            }
        }
    }

    Ok(())
}
