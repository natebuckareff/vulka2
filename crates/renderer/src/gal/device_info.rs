use vulkanalia::vk;

use crate::gal::{
    device_builder::DeviceKind,
    queue::{QueueCapFlags, QueueFamily},
};

#[derive(Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub kind: Option<DeviceKind>,
    pub families: Vec<QueueFamilyInfo>,
    pub physical_device: vk::PhysicalDevice,
}

#[derive(Debug)]
pub struct QueueFamilyInfo {
    family: QueueFamily,
    count: u32,
    flags: QueueCapFlags,
    present: bool,
}

impl QueueFamilyInfo {
    pub(crate) fn new(
        family: QueueFamily,
        count: u32,
        flags: QueueCapFlags,
        present: bool,
    ) -> Self {
        Self {
            family,
            count,
            flags,
            present,
        }
    }

    pub fn family(&self) -> QueueFamily {
        self.family
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn supports_graphics(&self) -> bool {
        self.flags.contains(QueueCapFlags::GRAPHICS)
    }

    pub fn supports_compute(&self) -> bool {
        self.flags.contains(QueueCapFlags::COMPUTE)
    }

    pub fn supports_transfer(&self) -> bool {
        self.flags.contains(QueueCapFlags::TRANSFER)
            || self.supports_graphics()
            || self.supports_compute()
    }

    pub fn supports_present(&self) -> bool {
        self.present
    }
}
