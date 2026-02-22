use anyhow::Result;

use crate::gpu_v2::{LaneVec, LivenessGuard, PoolLane, QueueGroupId, QueueId, QueueRoleFlags};

pub(crate) struct BufferLane {
    pub(crate) pool: PoolLane,
    pub(crate) dirty: bool,
}

// TODO: bring back?
// #[derive(Default)]
// struct DirtyBufferGuard {
//     dirty: bool,
// }

// impl Drop for DirtyBufferGuard {
//     fn drop(&mut self) {
//         debug_assert!(!self.dirty, "unsubmitted command buffer dropped");
//     }
// }

pub struct CommandBuffer {
    queue_group_id: QueueGroupId,
    lanes: LaneVec<BufferLane>,
    guard: LivenessGuard,
}

impl CommandBuffer {
    pub(crate) fn new(
        queue_group_id: QueueGroupId,
        pool_lanes: &LaneVec<PoolLane>,
        guard: LivenessGuard,
    ) -> Result<Self> {
        let mut lanes = LaneVec::new(queue_group_id, pool_lanes.len());
        for pool_lane in pool_lanes.iter() {
            let lane = BufferLane {
                pool: pool_lane.clone(),
                dirty: false,
            };
            lanes.push(lane);
        }
        Ok(Self {
            queue_group_id,
            lanes,
            guard,
        })
    }

    pub fn queue_group_id(&self) -> QueueGroupId {
        self.queue_group_id
    }

    pub(crate) fn lanes(&self) -> &LaneVec<BufferLane> {
        &self.lanes
    }

    // called by command recoding methods
    fn touch_by_id(&mut self, id: QueueId) {
        for lane in self.lanes.iter_mut() {
            if lane.pool.queue.id == id {
                lane.dirty = true;
            }
        }
    }

    // called by command recoding methods
    fn touch_by_roles(&mut self, roles: QueueRoleFlags) {
        for lane in self.lanes.iter_mut() {
            if lane.pool.queue.roles.contains(roles) {
                lane.dirty = true;
            }
        }
    }
}
