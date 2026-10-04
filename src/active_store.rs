use crate::{
    reactor::ReactorOutput,
    state::{PoolCell, PoolState},
    types::PoolId,
    update_queue::{DirtyPoolQueue, MarkDirtyResult},
};

pub struct ActivePoolStore {
    pools: Vec<Option<PoolCell>>,
    dirty: DirtyPoolQueue,
}

impl ActivePoolStore {
    pub fn new(pool_count: usize, dirty_capacity: usize) -> Self {
        Self {
            pools: (0..pool_count).map(|_| None).collect(),
            dirty: DirtyPoolQueue::new(pool_count, dirty_capacity),
        }
    }

    pub fn apply(&mut self, output: ReactorOutput) -> MarkDirtyResult {
        let pool_id = match output {
            ReactorOutput::PoolUpdated { pool_id, state } => {
                self.publish(pool_id, state);
                pool_id
            }
            ReactorOutput::PoolInvalidated { pool_id, .. } => {
                self.invalidate(pool_id);
                pool_id
            }
        };

        self.dirty.mark_dirty(pool_id)
    }

    pub fn publish(&mut self, pool_id: PoolId, state: PoolState) {
        let slot = self
            .pools
            .get_mut(pool_id as usize)
            .expect("pool id exceeds active store");

        match slot {
            Some(cell) => cell.replace(state),
            None => *slot = Some(PoolCell::new(state)),
        }
    }

    pub fn invalidate(&mut self, pool_id: PoolId) -> bool {
        self.pools
            .get_mut(pool_id as usize)
            .expect("pool id exceeds active store")
            .take()
            .is_some()
    }

    pub fn get(&self, pool_id: PoolId) -> Option<&PoolCell> {
        self.pools.get(pool_id as usize)?.as_ref()
    }

    pub fn is_ready(&self, pool_id: PoolId) -> bool {
        self.get(pool_id).is_some()
    }

    pub fn pop_dirty(&self) -> Option<PoolId> {
        self.dirty.pop()
    }

    pub fn dirty_len(&self) -> usize {
        self.dirty.len()
    }
}
