use vulkanalia::vk;

use crate::gpu_v2::{QueueId, QueueRoleFlags};

#[derive(Debug, Clone, Copy)]
pub struct Queue {
    id: QueueId,
    roles: QueueRoleFlags,
    handle: vk::Queue,
    // semaphore: vk::Semaphore,
}

impl Queue {
    pub(crate) fn new(id: QueueId, roles: QueueRoleFlags, handle: vk::Queue) -> Self {
        Self {
            id,
            roles,
            handle,
            // semaphore,
        }
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
