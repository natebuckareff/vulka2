use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;

pub struct SemaphoreResource {
    device: Arc<DeviceResource>,
    handle: vk::Semaphore,
}

impl SemaphoreResource {
    pub(crate) fn new(device: Arc<DeviceResource>) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let mut type_info = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        let create_info = vk::SemaphoreCreateInfo::builder().push_next(&mut type_info);
        let handle = unsafe { device.handle().create_semaphore(&create_info, None)? };
        Ok(Self { device, handle })
    }

    // XXX: Resource trait?
    pub(crate) unsafe fn handle(&self) -> vk::Semaphore {
        self.handle
    }
}

impl Drop for SemaphoreResource {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;

        unsafe {
            self.device.handle().destroy_semaphore(self.handle, None);
        }
    }
}
