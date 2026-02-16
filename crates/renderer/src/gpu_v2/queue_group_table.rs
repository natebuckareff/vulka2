use std::{collections::HashMap, sync::Arc};

use vulkanalia::vk;

use crate::gpu_v2::{QueueGroup, QueueGroupId, QueueId, QueueRoleFlags};

#[derive(Debug, Clone)]
pub struct QueueGroupInfo {
    pub id: QueueGroupId,
    pub bindings: Vec<QueueBinding>, // OPTIMIZE: SmallVec
}

#[derive(Debug, Clone, Copy)]
pub struct QueueBinding {
    pub id: QueueId,
    pub roles: QueueRoleFlags,
}

struct Inner {
    infos: Vec<QueueGroupInfo>,
    semaphores: HashMap<(u32, u32), vk::Semaphore>,
}

impl Inner {
    fn new(queue_groups: &HashMap<QueueGroupId, QueueGroup>) -> Self {
        let mut queue_count = 0;
        for qg in queue_groups.values() {
            queue_count += qg.queues().len();
        }
        let mut infos = Vec::with_capacity(queue_groups.len());
        let mut semaphores = HashMap::with_capacity(queue_count);
        for qg in queue_groups.values() {
            let mut info = QueueGroupInfo {
                id: qg.id(),
                bindings: Vec::with_capacity(qg.queues().len()),
            };
            for queue in qg.queues() {
                let binding = QueueBinding {
                    id: queue.id(),
                    roles: queue.roles(),
                };
                info.bindings.push(binding);
                let family: u32 = queue.id().family.into();
                semaphores.insert((family, queue.id().index), queue.semaphore());
            }
            infos.push(info);
        }
        infos.sort_by_key(|info| info.id);
        Self { infos, semaphores }
    }
}

#[derive(Clone)]
pub struct QueueGroupTable {
    inner: Arc<Inner>,
}

impl QueueGroupTable {
    pub fn new(queue_groups: &HashMap<QueueGroupId, QueueGroup>) -> Self {
        let inner = Inner::new(queue_groups);
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn get_info(&self, id: QueueGroupId) -> Option<&QueueGroupInfo> {
        self.inner.infos.iter().find(|info| info.id == id)
    }

    pub fn get_semaphore(&self, id: QueueId) -> Option<&vk::Semaphore> {
        self.inner.semaphores.get(&(id.family.into(), id.index))
    }
}
