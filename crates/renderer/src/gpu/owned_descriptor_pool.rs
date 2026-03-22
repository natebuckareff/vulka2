use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0};

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedDescriptorPool {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    pool: vk::DescriptorPool,
}

impl OwnedDescriptorPool {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::DescriptorPoolCreateInfoBuilder,
    ) -> Result<Self> {
        let pool = unsafe { device.raw().create_descriptor_pool(info, None) }?;
        Ok(Self { device, pool })
    }
}

impl VulkanResource for OwnedDescriptorPool {
    type Raw = vk::DescriptorPool;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.pool
    }
}

impl Drop for OwnedDescriptorPool {
    fn drop(&mut self) {
        unsafe {
            self.device.raw().destroy_descriptor_pool(self.pool, None);
        }
    }
}
