use anyhow::Result;

use crate::gpu_v2::{LivenessGuard, LivenessToken, PoolLanes, QueueGroupId, QueueGroupInfo};

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
