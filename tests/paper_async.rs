#![cfg(feature = "yellowstone")]

use hft_solana::{
    active_store::ActivePoolStore,
    feed::{AccountUpdate, FeedEvent},
    graph::GraphIndex,
    opportunity_engine::OpportunityEngine,
    paper::{async_loop::run_paper_event_loop, PaperPipeline},
    reactor::PaperStateReactor,
};
use tokio::sync::mpsc;

fn empty_pipeline() -> PaperPipeline {
    PaperPipeline::new(
        PaperStateReactor::new(4, 0),
        ActivePoolStore::new(0, 1),
        OpportunityEngine::new(GraphIndex::from_cycles(0, Vec::new()), Vec::new()),
    )
}

#[tokio::test]
async fn async_loop_records_event_processing_telemetry() {
    let (feed_tx, feed_rx) = mpsc::channel(4);
    let (opportunity_tx, mut opportunity_rx) = mpsc::channel(4);

    feed_tx
        .send(FeedEvent::Account(AccountUpdate {
            pubkey: [1u8; 32],
            owner: [2u8; 32],
            slot: 100,
            write_version: 1,
            generation: 1,
            bank_id: Some(9),
            is_startup: false,
            data: vec![7],
        }))
        .await
        .unwrap();
    drop(feed_tx);

    let (pipeline, stats) = run_paper_event_loop(feed_rx, opportunity_tx, empty_pipeline())
        .await
        .unwrap();

    assert_eq!(stats.events_processed, 1);
    assert_eq!(stats.event_process_ns_total, stats.event_process_ns_max);
    assert_eq!(stats.opportunities_forwarded, 0);
    assert_eq!(stats.opportunities_dropped, 0);
    assert_eq!(pipeline.stats().feed_events, 1);
    assert_eq!(pipeline.stats().pools_evaluated, 0);

    assert!(opportunity_rx.recv().await.is_none());
}
