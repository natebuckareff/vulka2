use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;

pub struct CommandPoolResource {
    handle: vk::CommandPool,
}

impl CommandPoolResource {
    pub(crate) fn new(device: &DeviceResource) -> Result<Self> {
        todo!()
    }
}
