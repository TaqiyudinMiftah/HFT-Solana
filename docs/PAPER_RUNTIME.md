# Paper runtime orchestration

The paper runtime connects the already-isolated components without introducing
a live transaction sender.

```text
Yellowstone FeedEvent
        |
        v
PaperStateReactor
  journal / fork rollback
        |
        v
ActivePoolStore
  ArcSwap latest state
        |
        v
DirtyPoolQueue
  bounded + coalesced
        |
        v
OpportunityEngine
  frozen cycle snapshot
  slot-skew check
  generation revalidation
        |
        v
Paper Opportunity
```

## Bounded queue correctness

A bounded queue is useful only if overflow does not silently lose a newly
published state. `PaperPipeline` therefore treats `QueueFull` as a
synchronous fallback:

1. publish the new pool state;
2. attempt to mark the pool dirty;
3. if the queue is full, evaluate that pool immediately;
4. drain the normal coalesced queue afterward.

The opportunity engine's per-cycle captured versions suppress duplicate work
when an overflow evaluation and a queued neighboring pool refer to the same
market state.

## Live-feed bridge

With the `yellowstone` feature enabled,
`paper::async_loop::run_paper_event_loop` consumes a bounded Tokio
`Receiver<FeedEvent>` and forwards paper opportunities through a second
bounded channel.

Opportunity forwarding uses `try_send`: a slow metrics/UI consumer is allowed
to drop paper opportunities rather than backpressuring the market-data feed.
The dropped count is explicit telemetry.

A caller can wire it to the existing Yellowstone adapter:

```text
run_account_feed(config, feed_tx)
              |
              v
         feed_rx
              |
              v
run_paper_event_loop(feed_rx, opportunity_tx, pipeline)
```

No signer, private key, Jito sender, Helius sender, or mainnet transaction
submission is present in this runtime.
