use anyhow::Result;

use crate::gpu_v2::{
    GpuFuture, LaneVec, LivenessGuard, LivenessToken, QueueGroupId, QueueGroupInfo, QueueId,
    QueueRoleFlags,
};

// TODO: what is this from again? is it pool-specific?
#[derive(Clone, Copy)]
pub(crate) struct QueueLane {
    pub(crate) id: QueueId,
    pub(crate) roles: QueueRoleFlags,
}

#[derive(Clone)]
pub(crate) struct PoolLane {
    pub(crate) queue: QueueLane,
    pub(crate) future: GpuFuture,
}

pub struct CommandPool {
    allocator_id: usize,
    queue_group_id: QueueGroupId,
    lanes: LaneVec<PoolLane>,
    liveness: LivenessToken,
    guard: LivenessGuard,
}

impl CommandPool {
    pub(crate) fn new(
        allocator_id: usize,
        queue_info: QueueGroupInfo,
        guard: LivenessGuard,
    ) -> Result<Self> {
        // TODO: something about this smells; do we need all this?
        let queue_group_id = queue_info.id;
        let mut lanes = LaneVec::new(queue_group_id, queue_info.bindings.len());
        for binding in queue_info.bindings.iter() {
            let queue = QueueLane {
                id: binding.id,
                roles: binding.roles,
            };
            let lane = PoolLane {
                queue,
                future: GpuFuture::new(),
            };
            lanes.push(lane);
        }
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

    pub(crate) fn lanes(&self) -> &LaneVec<PoolLane> {
        &self.lanes
    }

    pub(crate) fn liveness(&self) -> &LivenessToken {
        &self.liveness
    }

    pub(crate) fn reset(&mut self) -> Result<()> {
        for lane in self.lanes.iter_mut() {
            lane.future.reset()?;
        }
        Ok(())
    }
}
