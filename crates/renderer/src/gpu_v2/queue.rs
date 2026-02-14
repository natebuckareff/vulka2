use anyhow::{Result, anyhow};
use bitflags::bitflags;
use vulkanalia::vk;

use crate::gpu_v2::DeviceBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueFamilyId(u32);

impl From<u32> for QueueFamilyId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<usize> for QueueFamilyId {
    fn from(value: usize) -> Self {
        Self(value as u32)
    }
}

impl Into<u32> for QueueFamilyId {
    fn into(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueueId {
    pub family: QueueFamilyId,
    pub index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueGroupId(u32);

impl From<u32> for QueueGroupId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFamily {
    pub id: QueueFamilyId,
    pub roles: QueueRoleFlags,
    pub count: u32,
}

bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct QueueRoleFlags: u8 {
        const GRAPHICS = 0b0001;
        const COMPUTE  = 0b0010;
        const TRANSFER = 0b0100;
        const PRESENT  = 0b1000;
    }
}

impl From<vk::QueueFlags> for QueueRoleFlags {
    fn from(flags: vk::QueueFlags) -> Self {
        let mut roles = Self::empty();
        if flags.contains(vk::QueueFlags::GRAPHICS) {
            roles |= Self::GRAPHICS;
        }
        if flags.contains(vk::QueueFlags::COMPUTE) {
            roles |= Self::COMPUTE;
        }
        if flags.contains(vk::QueueFlags::TRANSFER) {
            roles |= Self::TRANSFER;
        }
        roles
    }
}

pub struct QueueGroupBuilder<'a> {
    builder: &'a mut DeviceBuilder,
    id: QueueGroupId,
    roles: QueueRoleFlags,
}

impl<'a> QueueGroupBuilder<'a> {
    pub(crate) fn new(builder: &'a mut DeviceBuilder, id: QueueGroupId) -> Self {
        Self {
            builder,
            id,
            roles: QueueRoleFlags::empty(),
        }
    }

    pub fn graphics(mut self) -> Self {
        self.roles |= QueueRoleFlags::GRAPHICS;
        self
    }

    pub fn present(mut self) -> Self {
        self.roles |= QueueRoleFlags::PRESENT;
        self
    }

    pub fn compute(mut self) -> Self {
        self.roles |= QueueRoleFlags::COMPUTE;
        self
    }

    pub fn transfer(mut self) -> Self {
        self.roles |= QueueRoleFlags::TRANSFER;
        self
    }

    pub fn build(self) -> Result<QueueGroupId> {
        let Some(group_id) = self.builder.allocate_group(self.id, self.roles)? else {
            return Err(anyhow!("no queue group found"));
        };
        Ok(group_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueAllocation {
    pub queue_id: QueueId,
    pub roles: QueueRoleFlags,
}

#[derive(Debug, Clone, Copy)]
pub struct Queue {
    id: QueueId,
    roles: QueueRoleFlags,
    handle: vk::Queue,
}

impl Queue {
    pub(crate) fn new(id: QueueId, roles: QueueRoleFlags, handle: vk::Queue) -> Self {
        Self { id, roles, handle }
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
}

#[derive(Debug)]
pub struct QueueGroup {
    id: QueueGroupId,
    queues: Vec<Queue>,
}

impl QueueGroup {
    pub(crate) fn new(id: QueueGroupId, queues: Vec<Queue>) -> Self {
        Self { id, queues }
    }

    pub fn id(&self) -> QueueGroupId {
        self.id
    }

    pub fn queues(&self) -> &[Queue] {
        &self.queues
    }
}
