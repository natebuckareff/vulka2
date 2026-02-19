use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu_v2::{QueueId, QueueRoleFlags, SubmissionId};

#[derive(Debug, Clone)]
pub struct Queue {
    id: QueueId,
    roles: QueueRoleFlags,
    handle: vk::Queue,
    submission_counter: Arc<SubmissionCounter>,
}

impl Queue {
    pub(crate) fn new(id: QueueId, roles: QueueRoleFlags, handle: vk::Queue) -> Self {
        Self {
            id,
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

    pub fn roles(&self) -> QueueRoleFlags {
        self.roles
    }

    pub fn handle(&self) -> vk::Queue {
        self.handle
    }

    pub fn semaphore(&self) -> vk::Semaphore {
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
