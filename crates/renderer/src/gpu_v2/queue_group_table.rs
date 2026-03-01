use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use vulkanalia::vk;

use crate::gpu_v2::{
    LaneIndex, LaneVec, LaneVecBuilder, QueueGroup, QueueGroupId, QueueId, QueueRoleFlags,
    VulkanHandle,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueLaneKey {
    id: QueueGroupId,
    index: u32, // TODO: also back in LaneIndex!
    key: u32,
}

impl QueueLaneKey {
    pub fn id(&self) -> QueueGroupId {
        self.id
    }

    // TODO: ok so sometimes we call this lane and sometimes we call it index
    // **pick something and stick to it**
    pub fn lane(&self) -> LaneIndex {
        LaneIndex {
            queue_group_id: self.id,
            index: self.index as usize, // XXX
        }
    }

    pub fn key(&self) -> u32 {
        self.key
    }
}

impl Default for QueueLaneKey {
    fn default() -> Self {
        Self {
            id: QueueGroupId::from(u32::MAX),
            index: u32::MAX,
            key: u32::MAX,
        }
    }
}

#[derive(Clone)]
pub struct QueueGroupInfo {
    pub id: QueueGroupId,
    pub offset: u32,
    pub bindings: LaneVec<QueueBinding>,
}

impl QueueGroupInfo {
    pub fn get_queue_group_lane(&self, index: LaneIndex) -> QueueLaneKey {
        assert!(index.queue_group_id() == self.id);
        assert!(index.index() < self.bindings.len());
        let key = self.offset + index.index() as u32;
        QueueLaneKey {
            id: self.id,
            index: index.index() as u32, // XXX
            key,
        }
    }
}

#[derive(Clone)]
pub struct QueueBinding {
    pub id: QueueId,
    pub roles: QueueRoleFlags,
    pub semaphore: VulkanHandle<vk::Semaphore>, // TODO: VulkanHandle
}

struct Inner {
    infos: Vec<QueueGroupInfo>,
    total_lanes: u32,
}

impl Inner {
    fn new(queue_groups: &HashMap<QueueGroupId, QueueGroup>) -> Self {
        let mut infos = Vec::with_capacity(queue_groups.len());
        let mut values = queue_groups.values().collect::<Vec<_>>();
        let mut offset = 0;
        values.sort_by_key(|qg| qg.id());
        for qg in values {
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
                offset,
                bindings: bindings.build(),
            };
            offset += info.bindings.len() as u32;
            infos.push(info);
        }
        infos.sort_by_key(|info| info.id);
        Self {
            infos,
            total_lanes: offset,
        }
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

    pub fn total_lanes(&self) -> u32 {
        self.inner.total_lanes
    }

    pub fn get_info(&self, id: QueueGroupId) -> Option<&QueueGroupInfo> {
        self.inner.infos.iter().find(|info| info.id == id)
    }

    pub fn get_binding(&self, lane: LaneIndex) -> Result<&QueueBinding> {
        self.inner
            .infos
            .iter()
            .find(|info| info.id == lane.queue_group_id())
            .map(|info| info.bindings.get(lane))
            .context("lane not found")
    }

    pub fn get_queue_group_lane(&self, lane: LaneIndex) -> Result<QueueLaneKey> {
        self.inner
            .infos
            .iter()
            .find(|info| info.id == lane.queue_group_id())
            .map(|info| info.get_queue_group_lane(lane))
            .context("queue group not found")
    }

    pub fn iter_bindings(&self) -> impl Iterator<Item = (LaneIndex, &QueueBinding)> {
        self.inner
            .infos
            .iter()
            .flat_map(|info| info.bindings.iter_entries())
    }
}
