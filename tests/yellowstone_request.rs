#![cfg(feature = "yellowstone")]

use hft_solana::feed::yellowstone::{
    build_subscribe_request, YellowstoneAccountFilter, YellowstoneConfig,
    YellowstoneMemcmpFilter, YellowstoneScopedAccountFilter,
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
