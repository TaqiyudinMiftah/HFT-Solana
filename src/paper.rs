use crate::{
    active_store::ActivePoolStore,
    feed::FeedEvent,
    opportunity::Opportunity,
    opportunity_engine::OpportunityEngine,
    reactor::{PaperStateReactor, ReactorOutput},
    types::PoolId,
    update_queue::MarkDirtyResult,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaperStats {
    pub feed_events: u64,
    pub reactor_updates: u64,
    pub reactor_invalidations: u64,
    pub dirty_queued: u64,
    pub dirty_collapsed: u64,
    pub queue_full_fallbacks: u64,
    pub pools_evaluated: u64,
    pub opportunities_emitted: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DlmmRefreshRequest {
    pub pool_id: PoolId,
    pub missing_accounts: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaperBatch {
    pub opportunities: Vec<Opportunity>,
    pub dlmm_refresh_requests: Vec<DlmmRefreshRequest>,
    pub queue_full_fallbacks: u64,
    pub pools_evaluated: u64,
}

/// Synchronous paper-trading orchestration core.
///
/// Network ingestion is deliberately kept outside this type. A FeedEvent is
/// journaled/rebuilt by the reactor, published to the active store, coalesced
/// through the bounded dirty queue, then evaluated by the opportunity engine.
///
/// If the dirty queue is full, the just-published pool is evaluated
/// immediately so bounded backpressure cannot silently lose the latest state.
pub struct PaperPipeline {
    reactor: PaperStateReactor,
    store: ActivePoolStore,
    engine: OpportunityEngine,
    stats: PaperStats,
}

impl PaperPipeline {
    pub fn new(
        reactor: PaperStateReactor,
        store: ActivePoolStore,
        engine: OpportunityEngine,
    ) -> Self {
        Self {
            reactor,
            store,
            engine,
            stats: PaperStats::default(),
        }
    }

    pub fn stats(&self) -> PaperStats {
        self.stats
    }

    pub fn reactor(&self) -> &PaperStateReactor {
        &self.reactor
    }

    pub fn store(&self) -> &ActivePoolStore {
        &self.store
    }

    pub fn engine(&self) -> &OpportunityEngine {
        &self.engine
    }

    pub fn process_event(&mut self, event: FeedEvent, created_ns: u64) -> PaperBatch {
        self.stats.feed_events = self.stats.feed_events.saturating_add(1);
        let outputs = self.reactor.process(event);
        self.process_outputs(outputs, created_ns)
    }

    /// Apply already-built reactor outputs.
    ///
    /// This is public primarily for deterministic replay/tests where raw
    /// account decoding is not the subject under test.
    pub fn process_outputs(&mut self, outputs: Vec<ReactorOutput>, created_ns: u64) -> PaperBatch {
        let mut batch = PaperBatch::default();
        let mut fallback_pools = Vec::<PoolId>::new();

        for output in outputs {
            let pool_id = output_pool_id(&output);

            #[cfg(feature = "meteora-dlmm")]
            if let ReactorOutput::PoolInvalidated {
                reason: crate::reactor::ReactorInvalidation::DlmmIncompleteBinWindow { missing },
                ..
            } = &output
            {
                batch.dlmm_refresh_requests.push(DlmmRefreshRequest {
                    pool_id,
                    missing_accounts: missing.clone(),
                });
            }

            match &output {
                ReactorOutput::PoolUpdated { .. } => {
                    self.stats.reactor_updates = self.stats.reactor_updates.saturating_add(1);
                }
                ReactorOutput::PoolInvalidated { .. } => {
                    self.stats.reactor_invalidations =
                        self.stats.reactor_invalidations.saturating_add(1);
                }
            }

            match self.store.apply(output) {
                MarkDirtyResult::Queued => {
                    self.stats.dirty_queued = self.stats.dirty_queued.saturating_add(1);
                }
                MarkDirtyResult::AlreadyDirty => {
                    self.stats.dirty_collapsed = self.stats.dirty_collapsed.saturating_add(1);
                }
                MarkDirtyResult::QueueFull => {
                    self.stats.queue_full_fallbacks =
                        self.stats.queue_full_fallbacks.saturating_add(1);
                    batch.queue_full_fallbacks = batch.queue_full_fallbacks.saturating_add(1);

                    if !fallback_pools.contains(&pool_id) {
                        fallback_pools.push(pool_id);
                    }
                }
            }
        }

        // A single feed event can rebuild several pools that share a config
        // account. Publish the whole batch before searching so an overflow
        // fallback can never observe a mixed old/new state vector.
        for pool_id in fallback_pools {
            self.evaluate_pool(pool_id, created_ns, &mut batch);
        }

        while let Some(pool_id) = self.store.pop_dirty() {
            self.evaluate_pool(pool_id, created_ns, &mut batch);
        }

        batch
    }

    fn evaluate_pool(&mut self, pool_id: PoolId, created_ns: u64, batch: &mut PaperBatch) {
        self.stats.pools_evaluated = self.stats.pools_evaluated.saturating_add(1);
        batch.pools_evaluated = batch.pools_evaluated.saturating_add(1);

        let opportunities = self.engine.process_pool(pool_id, &self.store, created_ns);
        self.stats.opportunities_emitted = self
            .stats
            .opportunities_emitted
            .saturating_add(opportunities.len() as u64);
        batch.opportunities.extend(opportunities);
    }
}

#[inline]
fn output_pool_id(output: &ReactorOutput) -> PoolId {
    match output {
        ReactorOutput::PoolUpdated { pool_id, .. }
        | ReactorOutput::PoolInvalidated { pool_id, .. } => *pool_id,
    }
}

#[cfg(feature = "yellowstone")]
pub mod async_loop {
    use std::time::{SystemTime, UNIX_EPOCH};

    use thiserror::Error;
    use tokio::sync::mpsc;

    use crate::{feed::FeedEvent, opportunity::Opportunity};

    use super::{DlmmRefreshRequest, PaperPipeline};

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct PaperOutputStats {
        pub opportunities_forwarded: u64,
        pub opportunities_dropped: u64,
        pub refresh_requests_forwarded: u64,
        pub refresh_requests_dropped: u64,
    }

    #[derive(Debug, Error)]
    pub enum PaperLoopError {
        #[error("system clock is before Unix epoch")]
        Clock,
    }

    /// Drain FeedEvents into the paper pipeline.
    ///
    /// Opportunity forwarding is intentionally non-blocking. If the consumer
    /// cannot keep up, paper opportunities are dropped and counted rather than
    /// applying backpressure to the market-data feed.
    pub async fn run_paper_event_loop(
        input: mpsc::Receiver<FeedEvent>,
        output: mpsc::Sender<Opportunity>,
        pipeline: PaperPipeline,
    ) -> Result<(PaperPipeline, PaperOutputStats), PaperLoopError> {
        run_paper_event_loop_inner(input, output, None, pipeline).await
    }

    pub async fn run_paper_event_loop_with_refresh(
        input: mpsc::Receiver<FeedEvent>,
        output: mpsc::Sender<Opportunity>,
        refresh_output: mpsc::Sender<DlmmRefreshRequest>,
        pipeline: PaperPipeline,
    ) -> Result<(PaperPipeline, PaperOutputStats), PaperLoopError> {
        run_paper_event_loop_inner(input, output, Some(refresh_output), pipeline).await
    }

    async fn run_paper_event_loop_inner(
        mut input: mpsc::Receiver<FeedEvent>,
        output: mpsc::Sender<Opportunity>,
        refresh_output: Option<mpsc::Sender<DlmmRefreshRequest>>,
        mut pipeline: PaperPipeline,
    ) -> Result<(PaperPipeline, PaperOutputStats), PaperLoopError> {
        let mut stats = PaperOutputStats::default();

        while let Some(event) = input.recv().await {
            let created_ns = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| PaperLoopError::Clock)?
                .as_nanos()
                .min(u64::MAX as u128) as u64;

            let batch = pipeline.process_event(event, created_ns);

            for request in batch.dlmm_refresh_requests {
                match refresh_output.as_ref() {
                    Some(output) => match output.try_send(request) {
                        Ok(()) => {
                            stats.refresh_requests_forwarded =
                                stats.refresh_requests_forwarded.saturating_add(1);
                        }
                        Err(_) => {
                            stats.refresh_requests_dropped =
                                stats.refresh_requests_dropped.saturating_add(1);
                        }
                    },
                    None => {
                        stats.refresh_requests_dropped =
                            stats.refresh_requests_dropped.saturating_add(1);
                    }
                }
            }

            for opportunity in batch.opportunities {
                match output.try_send(opportunity) {
                    Ok(()) => {
                        stats.opportunities_forwarded =
                            stats.opportunities_forwarded.saturating_add(1);
                    }
                    Err(_) => {
                        stats.opportunities_dropped = stats.opportunities_dropped.saturating_add(1);
                    }
                }
            }
        }

        Ok((pipeline, stats))
    }
}
