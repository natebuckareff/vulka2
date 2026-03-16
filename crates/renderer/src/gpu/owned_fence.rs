use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedFence {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    handle: vk::Fence,
}

impl OwnedFence {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::FenceCreateInfoBuilder,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let handle = unsafe { device.raw().create_fence(info, None)? };
        Ok(Self { device, handle })
    }
}

impl VulkanResource for OwnedFence {
    type Raw = vk::Fence;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.handle
    }
}

impl Drop for OwnedFence {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.device.raw().destroy_fence(self.handle, None);
        }
    }
}
