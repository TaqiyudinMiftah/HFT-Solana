#![cfg(feature = "yellowstone")]

use hft_solana::feed::yellowstone::{
    build_subscribe_request, YellowstoneAccountFilter, YellowstoneConfig,
};
use yellowstone_grpc_proto::prelude::CommitmentLevel;

#[test]
fn account_request_is_processed_and_exactly_filtered() {
    let config = YellowstoneConfig {
        endpoint: "https://example.invalid".to_owned(),
        x_token: None,
        filter_name: "hft".to_owned(),
        filter: YellowstoneAccountFilter {
            accounts: vec!["Account111".to_owned(), "Account222".to_owned()],
            owners: vec!["Owner111".to_owned()],
        },
    };

    let request = build_subscribe_request(&config);
    assert_eq!(
        request.commitment,
        Some(CommitmentLevel::Processed as i32)
    );

    let filter = request.accounts.get("hft").unwrap();
    assert_eq!(filter.account, config.filter.accounts);
    assert_eq!(filter.owner, config.filter.owners);
}
