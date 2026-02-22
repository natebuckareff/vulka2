use std::ops::{Deref, DerefMut};

use anyhow::Result;
use smallvec::SmallVec;

use crate::gpu_v2::{
    GpuFuture, LivenessGuard, LivenessToken, MAX_LANES, QueueBinding, QueueGroupId, QueueGroupInfo,
    QueueId, QueueRoleFlags,
};

#[derive(Clone, Copy)]
pub(crate) struct QueueLane {
    pub(crate) id: QueueId,
    pub(crate) roles: QueueRoleFlags,
}

#[derive(Clone)]
pub(crate) struct PoolLane {
    pub(crate) queue: QueueLane,
    pub(crate) roles: QueueRoleFlags,
    pub(crate) future: GpuFuture,
}

impl PoolLane {
    pub(crate) fn reset(&mut self) -> Result<()> {
        self.future.reset()?;
        Ok(())
    }
}

// TODO: not sure how I feel about this, feels over-engineered
#[derive(Clone)]
pub(crate) struct PoolLanes(SmallVec<[PoolLane; MAX_LANES]>);

impl PoolLanes {
    pub(crate) fn new(bindings: &[QueueBinding]) -> Result<Self> {
        let mut lanes = SmallVec::with_capacity(MAX_LANES);
        for binding in bindings {
            let queue = QueueLane {
                id: binding.id,
                roles: binding.roles,
            };
            let lane = PoolLane {
                queue,
                roles: binding.roles,
                future: GpuFuture::new(),
            };
            lanes.push(lane);
        }
        Ok(Self(lanes))
    }

    pub(crate) fn reset(&mut self) -> Result<()> {
        for lane in self.0.iter_mut() {
            lane.reset()?;
        }
        Ok(())
    }
}

// TODO: replace with better API
impl Deref for PoolLanes {
    type Target = [PoolLane];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// TODO: replace with better API
impl DerefMut for PoolLanes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct CommandPool {
    allocator_id: usize,
    queue_group_id: QueueGroupId,
    lanes: PoolLanes,
    liveness: LivenessToken,
    guard: LivenessGuard,
}

impl CommandPool {
    pub(crate) fn new(
        allocator_id: usize,
        queue_info: QueueGroupInfo,
        guard: LivenessGuard,
    ) -> Result<Self> {
        let lanes = PoolLanes::new(&queue_info.bindings)?;
        Ok(Self {
            allocator_id,
            queue_group_id: queue_info.id,
            lanes,
            liveness: LivenessToken::new(),
            guard,
        })
    }

    pub(crate) fn allocator_id(&self) -> usize {
        self.allocator_id
    }

    pub fn queue_group_id(&self) -> QueueGroupId {
        self.queue_group_id
    }

    pub(crate) fn lanes(&self) -> &PoolLanes {
        &self.lanes
    }

    pub(crate) fn liveness(&self) -> &LivenessToken {
        &self.liveness
    }

    pub(crate) fn reset(&mut self) -> Result<()> {
        self.lanes.reset()?;
        Ok(())
    }
}
