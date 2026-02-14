use bitflags::bitflags;
use vulkanalia::vk;

use crate::gpu_v2::DeviceBuilder;

#[derive(Debug, Clone, Copy, PartialEq)]
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

pub struct QueueId {
    pub family: QueueFamilyId,
    pub index: u32,
}

pub struct QueueFamily {
    pub id: QueueFamilyId,
    pub flags: QueueRoleFlags,
    pub count: u32,
}

pub enum QueueRole {
    Graphics,
    Compute,
    Transfer,
    Present,
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

impl Into<QueueRoleFlags> for QueueRole {
    fn into(self) -> QueueRoleFlags {
        match self {
            QueueRole::Graphics => QueueRoleFlags::GRAPHICS,
            QueueRole::Compute => QueueRoleFlags::COMPUTE,
            QueueRole::Transfer => QueueRoleFlags::TRANSFER,
            QueueRole::Present => QueueRoleFlags::PRESENT,
        }
    }
}

pub struct QueueGroupBuilder<'a> {
    builder: &'a mut DeviceBuilder,
    roles: QueueRoleFlags,
}

impl<'a> QueueGroupBuilder<'a> {
    pub(crate) fn new(builder: &'a mut DeviceBuilder) -> Self {
        Self {
            builder,
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

    pub fn build(self) -> Option<QueueGroup> {
        // TODO: copy the logic from the test.ts typescript file we prototyped
        Some(QueueGroup::new())
    }
}

pub struct QueueGroup {
    // TODO
}

impl QueueGroup {
    fn new() -> Self {
        Self {}
    }
}
