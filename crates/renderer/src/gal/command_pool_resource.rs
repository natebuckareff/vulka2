use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;
use crate::gal::queue::QueueFamily;

pub struct CommandPoolResource {
    device: Arc<DeviceResource>,
    family: QueueFamily,
    handle: vk::CommandPool,
}

impl CommandPoolResource {
    pub(crate) fn new(device: Arc<DeviceResource>, family: QueueFamily) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(u32::from(family))
            .flags(vk::CommandPoolCreateFlags::empty());
        let handle = unsafe { device.handle().create_command_pool(&info, None)? };
        Ok(Self {
            device,
            family,
            handle,
        })
    }

    pub(crate) fn family(&self) -> QueueFamily {
        self.family
    }

    pub(crate) fn device(&self) -> &Arc<DeviceResource> {
        &self.device
    }

    pub(crate) unsafe fn handle(&self) -> vk::CommandPool {
        self.handle
    }
}

impl Drop for CommandPoolResource {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.device.handle().destroy_command_pool(self.handle, None);
        }
    }
}
