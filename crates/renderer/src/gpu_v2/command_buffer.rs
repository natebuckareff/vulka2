use anyhow::Result;

use crate::gpu_v2::{
    LaneVec, LivenessGuard, PoolLane, QueueGroupId, QueueId, QueueRoleFlags, UsageToken,
};

pub(crate) struct BufferLane {
    pub(crate) pool: PoolLane,
    pub(crate) dirty: bool,
}

pub struct CommandBuffer {
    queue_group_id: QueueGroupId,
    lanes: LaneVec<BufferLane>,
    guard: LivenessGuard,
    usage: Option<UsageToken>,
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
            usage: None,
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
                if self.usage.is_none() {
                    self.usage = Some(UsageToken::new());
                }
            }
        }
    }

    // called by command recoding methods
    fn touch_by_roles(&mut self, roles: QueueRoleFlags) {
        for lane in self.lanes.iter_mut() {
            if lane.pool.queue.roles.contains(roles) {
                lane.dirty = true;
                if self.usage.is_none() {
                    self.usage = Some(UsageToken::new());
                }
            }
        }
    }

    pub(crate) fn disarm(&mut self) {
        if let Some(mut usage) = self.usage.take() {
            usage.disarm();
        }
    }
}
