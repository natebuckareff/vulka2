use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;
use crate::gal::queue::QueueFamily;

pub struct QueueResource {
    family: QueueFamily,
    id: u32,
    handle: vk::Queue,
}

impl QueueResource {
    pub(crate) fn new(device: &DeviceResource, family: QueueFamily, id: u32) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let handle = unsafe { device.handle().get_device_queue(u32::from(family), id) };
        Ok(Self { family, id, handle })
    }

    pub(crate) fn family(&self) -> QueueFamily {
        self.family
    }

    pub(crate) fn id(&self) -> u32 {
        self.id
    }

    pub(crate) unsafe fn handle(&self) -> vk::Queue {
        self.handle
    }
}
