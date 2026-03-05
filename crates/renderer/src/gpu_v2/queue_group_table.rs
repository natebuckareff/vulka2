// TODO: this file needs a refactor in conjunction with lane_index, lane_vec,
// and queue_group_vec

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use vulkanalia::vk;

use crate::gpu_v2::{
    LaneVec, LaneVecBuilder, QueueGroup, QueueGroupId, QueueId, QueueRoleFlags, VulkanHandle,
};

// TODO: move, to lane_key.rs and harden safety

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LaneKey {
    offset: u16, // lane offset - max 65535 total lanes
    group: u8,   // max 255 groups
    index: u8,   // max 255 lanes per group
}

impl LaneKey {
    pub fn queue_group(&self) -> QueueGroupId {
        QueueGroupId::from(self.group)
    }

    pub fn index(&self) -> usize {
        self.index as usize
    }
}

impl From<(QueueGroupId, usize)> for LaneKey {
    fn from(value: (QueueGroupId, usize)) -> Self {
        Self {
            offset: 0,
            group: value.0.into(),
            index: value.1 as u8,
        }
    }
}

impl Into<u32> for LaneKey {
    fn into(self) -> u32 {
        (self.offset as u32) + (self.index as u32)
    }
}

impl Into<usize> for LaneKey {
    fn into(self) -> usize {
        let n: u32 = self.into();
        n as usize
    }
}

impl Default for LaneKey {
    fn default() -> Self {
        Self {
            offset: u16::MAX,
            group: u8::MAX,
            index: u8::MAX,
        }
    }
}

#[derive(Clone)]
pub struct QueueGroupInfo {
    pub id: QueueGroupId,
    pub offset: u16,
    pub bindings: LaneVec<QueueBinding>,
}

impl QueueGroupInfo {
    pub fn get_queue_lane_key(&self, index: usize) -> LaneKey {
        assert!(index < self.bindings.len());
        LaneKey {
            offset: self.offset,
            group: self.id.into(),
            index: index as u8,
        }
    }
}

#[derive(Clone)]
pub struct QueueBinding {
    pub id: QueueId,
    pub key: LaneKey,
    pub roles: QueueRoleFlags,
    pub semaphore: VulkanHandle<vk::Semaphore>, // TODO: VulkanHandle
}

struct Inner {
    infos: Vec<QueueGroupInfo>,
    total_lanes: u16,
}

impl Inner {
    fn new(queue_groups: &HashMap<QueueGroupId, QueueGroup>) -> Self {
        let mut infos = Vec::with_capacity(queue_groups.len());
        let mut values = queue_groups.values().collect::<Vec<_>>();
        let mut offset = 0;
        values.sort_by_key(|qg| qg.id());
        for qg in values {
            let mut bindings = LaneVecBuilder::with_lanes(qg.queues());
            for (i, queue) in qg.queues().iter().enumerate() {
                // XXX TODO: is this correct? Really need better validation
                let key = LaneKey {
                    offset,
                    group: qg.id().into(),
                    index: i as u8,
                };
                let binding = QueueBinding {
                    id: queue.id(),
                    key,
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
            assert!(offset <= u16::MAX);
            offset += info.bindings.len() as u16;
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

    pub fn total_lanes(&self) -> u16 {
        self.inner.total_lanes
    }

    pub fn get_info(&self, id: QueueGroupId) -> Option<&QueueGroupInfo> {
        self.inner.infos.iter().find(|info| info.id == id)
    }

    pub fn get_nth_binding(&self, n: usize) -> Option<&QueueBinding> {
        let mut i = 0;
        for info in self.inner.infos.iter() {
            for binding in info.bindings.iter() {
                if i == n {
                    return Some(binding);
                }
                i += 1;
            }
        }
        None
    }

    pub fn get_binding(&self, key: LaneKey) -> Result<&QueueBinding> {
        self.inner
            .infos
            .iter()
            .find(|info| info.id == key.queue_group())
            .map(|info| info.bindings.get(key))
            .context("lane not found")
    }

    pub fn iter_bindings(&self) -> impl Iterator<Item = (LaneKey, &QueueBinding)> {
        self.inner.infos.iter().flat_map(|info| {
            info.bindings
                .iter_entries()
                .map(|(key, binding)| (key, binding))
        })
    }
}
