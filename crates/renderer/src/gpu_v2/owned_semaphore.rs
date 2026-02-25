use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::{VulkanHandle, VulkanResource};

pub struct OwnedSemaphore {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    handle: vk::Semaphore,
}

impl OwnedSemaphore {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::SemaphoreCreateInfoBuilder,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let handle = unsafe { device.raw().create_semaphore(info, None)? };
        Ok(Self { device, handle })
    }
}

impl VulkanResource for OwnedSemaphore {
    type Raw = vk::Semaphore;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.handle
    }
}

impl Drop for OwnedSemaphore {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.device.raw().destroy_semaphore(self.handle, None);
        }
    }
}
