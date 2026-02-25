use std::{collections::HashMap, sync::Arc};

use vulkanalia::vk;

use crate::gpu_v2::{
    LaneVec, LaneVecBuilder, QueueGroup, QueueGroupId, QueueId, QueueRoleFlags, VulkanHandle,
};

#[derive(Clone)]
pub struct QueueGroupInfo {
    pub id: QueueGroupId,
    pub bindings: LaneVec<QueueBinding>,
}

#[derive(Clone)]
pub struct QueueBinding {
    pub id: QueueId,
    pub roles: QueueRoleFlags,
    pub semaphore: VulkanHandle<vk::Semaphore>, // TODO: VulkanHandle
}

struct Inner {
    infos: Vec<QueueGroupInfo>,
}

impl Inner {
    fn new(queue_groups: &HashMap<QueueGroupId, QueueGroup>) -> Self {
        let mut infos = Vec::with_capacity(queue_groups.len());
        for qg in queue_groups.values() {
            let mut bindings = LaneVecBuilder::with_lanes(qg.queues());
            for queue in qg.queues().iter() {
                let binding = QueueBinding {
                    id: queue.id(),
                    roles: queue.roles(),
                    semaphore: queue.semaphore().clone(),
                };
                bindings.push(binding);
            }
            let info = QueueGroupInfo {
                id: qg.id(),
                bindings: bindings.build(),
            };
            infos.push(info);
        }
        infos.sort_by_key(|info| info.id);
        Self { infos }
    }
}

#[derive(Clone)]
pub struct QueueGroupTable {
    inner: Arc<Inner>,
}

impl QueueGroupTable {
    pub(crate) fn new(queue_groups: &HashMap<QueueGroupId, QueueGroup>) -> Self {
        let inner = Inner::new(queue_groups);
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn get_info(&self, id: QueueGroupId) -> Option<&QueueGroupInfo> {
        self.inner.infos.iter().find(|info| info.id == id)
    }
}
