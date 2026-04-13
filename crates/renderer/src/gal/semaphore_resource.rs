use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;

pub struct SemaphoreResource {
    device: Arc<DeviceResource>,
    handle: vk::Semaphore,
}

impl SemaphoreResource {
    pub(crate) fn binary(device: Arc<DeviceResource>) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let create_info = vk::SemaphoreCreateInfo::builder();
        Self::new(device, create_info)
    }

    pub(crate) fn timeline(device: Arc<DeviceResource>, initial_value: u64) -> Result<Self> {
        use vulkanalia::prelude::v1_2::*;

        let mut type_info = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(initial_value);
        let create_info = vk::SemaphoreCreateInfo::builder().push_next(&mut type_info);
        Self::new(device, create_info)
    }

    fn new(device: Arc<DeviceResource>, info: vk::SemaphoreCreateInfoBuilder) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let handle = unsafe { device.handle().create_semaphore(&info, None)? };
        Ok(Self { device, handle })
    }

    pub(crate) unsafe fn handle(&self) -> vk::Semaphore {
        self.handle
    }

    pub(crate) fn device(&self) -> &Arc<DeviceResource> {
        &self.device
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
