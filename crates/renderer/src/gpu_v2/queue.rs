use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::{
    GpuFutureWriter, LaneIndex, QueueId, QueuePacket, QueueRoleFlags, SubmissionId,
};

#[derive(Debug, Clone)]
pub struct Queue {
    id: QueueId,
    lane: LaneIndex,
    roles: QueueRoleFlags,
    handle: vk::Queue,
    submission_counter: Arc<SubmissionCounter>,
}

impl Queue {
    pub(crate) fn new(
        id: QueueId,
        lane: LaneIndex,
        roles: QueueRoleFlags,
        handle: vk::Queue,
    ) -> Self {
        Self {
            id,
            lane,
            roles,
            handle,
            submission_counter: Arc::new(SubmissionCounter::new(id)),
        }
    }

    pub(crate) fn submission_counter(&self) -> &Arc<SubmissionCounter> {
        &self.submission_counter
    }

    pub fn id(&self) -> QueueId {
        self.id
    }

    pub(crate) fn lane(&self) -> LaneIndex {
        self.lane
    }

    pub fn roles(&self) -> QueueRoleFlags {
        self.roles
    }

    pub fn handle(&self) -> vk::Queue {
        self.handle
    }

    pub fn semaphore(&self) -> vk::Semaphore {
        todo!()
    }

    pub fn submit(&mut self, future: GpuFutureWriter, packets: &[QueuePacket]) -> Result<()> {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SubmissionCounter {
    id: QueueId,
    counter: Arc<AtomicU64>,
}

impl SubmissionCounter {
    pub(crate) fn new(id: QueueId) -> Self {
        Self {
            id,
            counter: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn reserve(&self) -> Result<SubmissionId> {
        let value = self.counter.fetch_add(1, Ordering::Release);
        SubmissionId::new(value + 1)
    }
}
