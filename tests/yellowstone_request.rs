#![cfg(feature = "yellowstone")]

use hft_solana::feed::yellowstone::{
    build_subscribe_request, provider_error_summary, YellowstoneAccountFilter, YellowstoneConfig,
    YellowstoneMemcmpFilter, YellowstoneScopedAccountFilter,
};
use yellowstone_grpc_proto::prelude::CommitmentLevel;

#[tokio::test]
async fn https_feed_sends_a_tls_handshake_before_any_subscription() {
    use std::time::Duration;

    use hft_solana::feed::yellowstone::run_account_feed;
    use tokio::{io::AsyncReadExt, net::TcpListener, sync::mpsc, time::timeout};

    // Observe the wire without installing a certificate or disabling validation.
    // The listener closes after ClientHello; a transport error is then expected.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = YellowstoneConfig {
        endpoint: format!("https://{}", listener.local_addr().unwrap()),
        x_token: Some("TLS_TEST_TOKEN".to_owned()),
        filter_name: "tls-test".to_owned(),
        filter: YellowstoneAccountFilter {
            accounts: vec!["Account111".to_owned()],
            ..Default::default()
        },
    };
    let (sender, _receiver) = mpsc::channel(1);
    let feed = tokio::spawn(run_account_feed(config, sender));
    let header = timeout(Duration::from_secs(5), async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut header = [0u8; 5];
        socket
            .read_exact(&mut header)
            .await
            .expect("HTTPS connection closed without a TLS record");
        header
    })
    .await
    .expect("HTTPS feed must initiate TLS promptly");
    assert_eq!(header[0], 0x16, "expected TLS handshake record");
    assert_eq!(header[1], 0x03, "expected TLS record version");
    let error = timeout(Duration::from_secs(5), feed)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(!error.to_string().contains("TLS_TEST_TOKEN"));
}

#[test]
fn provider_failures_explain_the_status_without_echoing_secrets() {
    use yellowstone_grpc_client::GeyserGrpcClientError;
    use yellowstone_grpc_proto::tonic::{Code, Status};

    for (code, hint) in [
        (Code::Unimplemented, "Yellowstone gRPC endpoint"),
        (Code::Unauthenticated, "authentication rejected"),
        (Code::PermissionDenied, "access denied"),
        (Code::ResourceExhausted, "quota exhausted"),
        (Code::FailedPrecondition, "bank metadata"),
    ] {
        let error = GeyserGrpcClientError::TonicStatus(Status::new(
            code,
            "https://provider.invalid/v2/SECRET_KEY x-token: SECRET_TOKEN",
        ));
        let summary = provider_error_summary(&error);
        assert!(summary.contains(hint));
        assert!(!summary.contains("SECRET"));
        assert!(!summary.contains("provider.invalid"));
    }
}

#[test]
fn account_request_is_processed_and_exactly_filtered() {
    let config = YellowstoneConfig {
        endpoint: "https://example.invalid".to_owned(),
        x_token: None,
        filter_name: "hft".to_owned(),
        filter: YellowstoneAccountFilter {
            accounts: vec!["Account111".to_owned(), "Account222".to_owned()],
            owners: vec!["Owner111".to_owned()],
            scoped: Vec::new(),
        },
    };

    let request = build_subscribe_request(&config);
    assert_eq!(request.commitment, Some(CommitmentLevel::Processed as i32));

    let filter = request.accounts.get("hft").unwrap();
    assert_eq!(filter.account, config.filter.accounts);
    assert_eq!(filter.owner, config.filter.owners);

    let slots = request.slots.get("slot_fence").unwrap();
    assert_eq!(slots.filter_by_commitment, Some(false));
    assert_eq!(slots.interslot_updates, Some(true));
}

#[test]
fn scoped_account_filter_builds_owner_and_memcmp_predicates() {
    let config = YellowstoneConfig {
        endpoint: "https://example.invalid".to_owned(),
        x_token: None,
        filter_name: "hft".to_owned(),
        filter: YellowstoneAccountFilter {
            accounts: vec!["Account111".to_owned()],
            owners: Vec::new(),
            scoped: vec![YellowstoneScopedAccountFilter {
                name: "dlmm-bin-0".to_owned(),
                owners: vec!["LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo".to_owned()],
                memcmp: vec![
                    YellowstoneMemcmpFilter {
                        offset: 0,
                        bytes: vec![92, 142, 92, 220, 5, 148, 70, 181],
                    },
                    YellowstoneMemcmpFilter {
                        offset: 24,
                        bytes: vec![7u8; 32],
                    },
                ],
            }],
        },
    };

    let request = build_subscribe_request(&config);
    let filter = request.accounts.get("hft:dlmm-bin-0").unwrap();

    assert_eq!(
        filter.owner,
        vec!["LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo".to_owned()]
    );
    assert_eq!(filter.filters.len(), 2);

    use yellowstone_grpc_proto::prelude::{
        subscribe_request_filter_accounts_filter::Filter,
        subscribe_request_filter_accounts_filter_memcmp::Data,
    };

    match filter.filters[0].filter.as_ref().unwrap() {
        Filter::Memcmp(memcmp) => {
            assert_eq!(memcmp.offset, 0);
            assert_eq!(
                memcmp.data,
                Some(Data::Bytes(vec![92, 142, 92, 220, 5, 148, 70, 181]))
            );
        }
        other => panic!("unexpected filter: {other:?}"),
    }

    match filter.filters[1].filter.as_ref().unwrap() {
        Filter::Memcmp(memcmp) => {
            assert_eq!(memcmp.offset, 24);
            assert_eq!(memcmp.data, Some(Data::Bytes(vec![7u8; 32])));
        }
        other => panic!("unexpected filter: {other:?}"),
    }
}
