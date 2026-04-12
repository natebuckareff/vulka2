use std::sync::Arc;

use anyhow::Result;
use bitflags::bitflags;

use crate::gal::{
    command_pool::Submission, device::DeviceResource, device_builder::QueueKind,
    queue_resource::QueueResource, semaphore_resource::SemaphoreResource,
};

pub struct Queue {
    device: Arc<DeviceResource>,
    semaphore: Arc<SemaphoreResource>,
    resource: QueueResource,
    kind: QueueKind,
    present: bool,
    lane: Lane,
}

impl Queue {
    pub(crate) fn new(
        device: Arc<DeviceResource>,
        semaphore: Arc<SemaphoreResource>,
        resource: QueueResource,
        kind: QueueKind,
        present: bool,
        lane: Lane,
    ) -> Self {
        Self {
            device,
            semaphore,
            resource,
            kind,
            present,
            lane,
        }
    }

    pub(crate) fn semaphore(&self) -> &Arc<SemaphoreResource> {
        &self.semaphore
    }

    fn id(&self) -> u32 {
        self.resource.id()
    }

    pub fn kind(&self) -> QueueKind {
        self.kind
    }

    pub fn presentable(&self) -> bool {
        self.present
    }

    fn lane(&self) -> Lane {
        self.lane
    }

    // fn timeline(&self) -> Result<TimelineValue> {
    //     todo!()
    // }

    fn submit(&mut self, submission: Submission) -> Result<()> {
        todo!()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Lane {
    index: LaneIndex,
    family: QueueFamily,
}

impl Lane {
    pub(crate) fn new(index: LaneIndex, family: QueueFamily) -> Self {
        Self { index, family }
    }

    pub(crate) fn index(&self) -> LaneIndex {
        self.index
    }

    pub(crate) fn family(&self) -> QueueFamily {
        self.family
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LaneIndex(u32);

impl LaneIndex {
    pub(crate) fn new(index: u32) -> Self {
        assert!(index < 64);
        Self(index)
    }
}

impl From<LaneIndex> for u32 {
    fn from(value: LaneIndex) -> Self {
        value.0
    }
}

impl From<LaneIndex> for usize {
    fn from(value: LaneIndex) -> Self {
        value.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFamily(u32);

impl From<u32> for QueueFamily {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<QueueFamily> for u32 {
    fn from(value: QueueFamily) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueueCap {
    Graphics,
    Compute,
    Transfer,
}

impl From<QueueCap> for char {
    fn from(value: QueueCap) -> Self {
        match value {
            QueueCap::Graphics => 'g',
            QueueCap::Compute => 'c',
            QueueCap::Transfer => 't',
        }
    }
}

bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct QueueCapFlags: u8 {
        const GRAPHICS = 0b001;
        const COMPUTE  = 0b010;
        const TRANSFER = 0b100;
    }
}

impl From<QueueCap> for QueueCapFlags {
    fn from(value: QueueCap) -> Self {
        match value {
            QueueCap::Graphics => QueueCapFlags::GRAPHICS,
            QueueCap::Compute => QueueCapFlags::COMPUTE,
            QueueCap::Transfer => QueueCapFlags::TRANSFER,
        }
    }
}

impl From<vulkanalia::vk::QueueFlags> for QueueCapFlags {
    fn from(flags: vulkanalia::vk::QueueFlags) -> Self {
        let mut caps = Self::empty();
        if flags.contains(vulkanalia::vk::QueueFlags::GRAPHICS) {
            caps |= Self::GRAPHICS;
        }
        if flags.contains(vulkanalia::vk::QueueFlags::COMPUTE) {
            caps |= Self::COMPUTE;
        }
        if flags.contains(vulkanalia::vk::QueueFlags::TRANSFER) {
            caps |= Self::TRANSFER;
        }
        caps
    }
}
