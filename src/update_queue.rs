use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_queue::ArrayQueue;

use crate::types::PoolId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkDirtyResult {
    Queued,
    AlreadyDirty,
    QueueFull,
}

/// Bounded dirty-pool scheduler.
///
/// The latest pool state lives elsewhere (normally ArcSwap). This queue only
/// carries PoolId, so many updates to the same pool collapse into one pending
/// search pass.
pub struct DirtyPoolQueue {
    queue: ArrayQueue<PoolId>,
    dirty: Box<[AtomicBool]>,
}

impl DirtyPoolQueue {
    pub fn new(pool_count: usize, capacity: usize) -> Self {
        assert!(capacity > 0, "dirty queue capacity must be nonzero");

        Self {
            queue: ArrayQueue::new(capacity),
            dirty: (0..pool_count)
                .map(|_| AtomicBool::new(false))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }

    pub fn mark_dirty(&self, pool: PoolId) -> MarkDirtyResult {
        let Some(flag) = self.dirty.get(pool as usize) else {
            return MarkDirtyResult::QueueFull;
        };

        if flag
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return MarkDirtyResult::AlreadyDirty;
        }

        if self.queue.push(pool).is_err() {
            flag.store(false, Ordering::Release);
            return MarkDirtyResult::QueueFull;
        }

        MarkDirtyResult::Queued
    }

    /// Pop one pool and clear its dirty flag before the caller loads state.
    ///
    /// If a new update arrives after the clear it will enqueue another pass.
    /// If it arrived before the clear, the caller's subsequent state load sees
    /// that already-published latest snapshot.
    pub fn pop(&self) -> Option<PoolId> {
        let pool = self.queue.pop()?;
        if let Some(flag) = self.dirty.get(pool as usize) {
            flag.store(false, Ordering::Release);
        }
        Some(pool)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
