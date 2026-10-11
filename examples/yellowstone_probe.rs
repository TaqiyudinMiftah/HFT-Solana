//! A bounded slot-only subscription check, independent of pool configuration.

use std::{env, error::Error, io, time::Duration};

use futures::StreamExt;
use hft_solana::feed::yellowstone::provider_error_summary;
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient, GeyserGrpcClientError};
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, CommitmentLevel, SlotStatus, SubscribeRequest,
    SubscribeRequestFilterSlots,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let endpoint = env::var("HFT_YELLOWSTONE_ENDPOINT")?;
    let token = env::var("HFT_YELLOWSTONE_X_TOKEN")
        .ok()
        .filter(|value| !value.is_empty());
    let timeout = Duration::from_secs(10);
    let https = endpoint
        .split_once(':')
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("https"));
    let mut builder = GeyserGrpcClient::build_from_shared(endpoint)
        .map_err(|_| io::Error::other("invalid gRPC endpoint URI; value omitted"))?
        .x_token(token)
        .map_err(|_| io::Error::other("invalid x-token metadata; value omitted"))?
        .connect_timeout(timeout);
    if https {
        builder = builder
            .tls_config(ClientTlsConfig::new().with_native_roots())
            .map_err(|_| io::Error::other("TLS configuration failed; details omitted"))?;
    }
    let mut client = tokio::time::timeout(timeout, builder.connect())
        .await
        .map_err(|_| io::Error::other("gRPC connection timed out"))?
        .map_err(|_| io::Error::other("gRPC connection/TLS failed; details omitted"))?;
    let request = SubscribeRequest {
        slots: [(
            "paper_probe".to_owned(),
            SubscribeRequestFilterSlots {
                filter_by_commitment: Some(false),
                interslot_updates: Some(true),
            },
        )]
        .into(),
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    };
    let (_sink, mut stream) =
        tokio::time::timeout(timeout, client.subscribe_with_request(Some(request)))
            .await
            .map_err(|_| io::Error::other("gRPC subscription timed out"))?
            .map_err(|error| io::Error::other(provider_error_summary(&error)))?;
    println!("YELLOWSTONE_PROBE subscription=accepted filter=slots_only");
    tokio::time::timeout(timeout, async {
        while let Some(update) = stream.next().await {
            let update = update.map_err(|error| {
                io::Error::other(provider_error_summary(&GeyserGrpcClientError::TonicStatus(error)))
            })?;
            if let Some(UpdateOneof::Slot(slot)) = update.update_oneof {
                if slot.status == SlotStatus::SlotProcessed as i32 {
                    let bank_id = slot.bank_id.ok_or_else(|| {
                        io::Error::other(
                            "processed slot has no bank_id; this runtime requires bank metadata",
                        )
                    })?;
                    println!(
                        "YELLOWSTONE_PROBE processed_slot={} bank_id={} account_ordering_and_recovery=not_verified",
                        slot.slot, bank_id
                    );
                    return Ok::<_, io::Error>(());
                }
            }
        }
        Err(io::Error::other("gRPC stream ended before a processed slot"))
    })
    .await
    .map_err(|_| io::Error::other("no processed slot received within 10s"))??;
    Ok(())
}
