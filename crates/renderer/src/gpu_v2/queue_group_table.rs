use std::{collections::HashMap, sync::Arc};

use vulkanalia::vk;

use crate::gpu_v2::{QueueGroup, QueueGroupId, QueueId};

// need: QueueGroupId -> QueueId
// need: QueueId -> vk::Semaphore

struct Inner {
    queue_groups: Vec<(QueueGroupId, QueueId)>,
    semaphores: Vec<(u32, u32, vk::Semaphore)>,
}

impl Inner {
    fn new(queue_groups: &HashMap<QueueGroupId, QueueGroup>) -> Self {
        let mut queue_count = 0;
        for qg in queue_groups.values() {
            queue_count += qg.queues().len();
        }

        let mut inner_queue_groups = Vec::with_capacity(queue_count);
        let mut semaphores = Vec::with_capacity(queue_count);

        for qg in queue_groups.values() {
            for queue in qg.queues() {
                inner_queue_groups.push((qg.id(), queue.id()));
                let family: u32 = queue.id().family.into();
                semaphores.push((family, queue.id().index, queue.semaphore()));
            }
        }

        inner_queue_groups.sort();
        semaphores.sort_by_key(|(family, queue, _)| (*family, *queue));

        Self {
            queue_groups: inner_queue_groups,
            semaphores,
        }
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

    pub fn get_queue_ids(&self, id: QueueGroupId) -> impl Iterator<Item = QueueId> {
        let start = self
            .inner
            .queue_groups
            .partition_point(|(group_id, _)| *group_id < id);

        let len = self.inner.queue_groups.len();
        let mut end = start;
        while end < len && self.inner.queue_groups[end].0 == id {
            end += 1;
        }

        self.inner.queue_groups[start..end]
            .iter()
            .map(|(_, queue_id)| *queue_id)
    }

    pub fn get_semaphore(&self, id: QueueId) -> Option<&vk::Semaphore> {
        let result = self
            .inner
            .semaphores
            .binary_search_by(|(family, queue, _)| {
                family
                    .cmp(&id.family.into())
                    .then_with(|| queue.cmp(&id.index))
            });

        match result {
            Ok(index) => Some(&self.inner.semaphores[index].2),
            Err(_) => None,
        }
    }
}
