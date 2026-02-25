use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::{VulkanHandle, VulkanResource};

pub struct OwnedCommandPool {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    pool: vk::CommandPool,
}

impl OwnedCommandPool {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::CommandPoolCreateInfoBuilder,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let pool = unsafe { device.raw().create_command_pool(info, None)? };
        Ok(Self { device, pool })
    }
}

impl VulkanResource for OwnedCommandPool {
    type Raw = vk::CommandPool;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.pool
    }
}

impl Drop for OwnedCommandPool {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.device.raw().destroy_command_pool(self.pool, None);
        }
    }
}
