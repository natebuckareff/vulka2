use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::{
    LaneVec, LivenessGuard, QueueGroupId, QueueId, QueueLane, QueueRoleFlags, UsageToken,
};

pub(crate) struct BufferLane {
    pub(crate) queue: QueueLane,
    pub(crate) dirty: bool,
    pub(crate) cmdbuf: vk::CommandBuffer,
}

impl BufferLane {
    pub(crate) fn new(queue: QueueLane, cmdbuf: vk::CommandBuffer) -> Self {
        Self {
            queue,
            dirty: false,
            cmdbuf,
        }
    }
}

pub struct CommandBuffer {
    lanes: LaneVec<BufferLane>,
    guard: LivenessGuard,
    usage: Option<UsageToken>,
}

impl CommandBuffer {
    pub(crate) fn new(lanes: LaneVec<BufferLane>, guard: LivenessGuard) -> Result<Self> {
        Ok(Self {
            lanes,
            guard,
            usage: None,
        })
    }

    pub fn queue_group_id(&self) -> QueueGroupId {
        self.lanes.queue_group_id()
    }

    pub(crate) fn lanes(&self) -> &LaneVec<BufferLane> {
        &self.lanes
    }

    // called by command recoding methods
    fn touch_by_id(&mut self, id: QueueId) {
        for lane in self.lanes.iter_mut() {
            if lane.queue.id == id {
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
            if lane.queue.roles.contains(roles) {
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
