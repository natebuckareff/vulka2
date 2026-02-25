use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::{VulkanHandle, VulkanResource};

pub struct OwnedDevice {
    handle: Arc<vulkanalia::Device>,
}

impl OwnedDevice {
    pub fn new(
        instance: VulkanHandle<Arc<vulkanalia::Instance>>,
        physical_device: vk::PhysicalDevice,
        info: &vk::DeviceCreateInfoBuilder,
    ) -> Result<Self> {
        let handle = unsafe { instance.raw().create_device(physical_device, &info, None)? };
        let handle = Arc::new(handle);
        Ok(Self { handle })
    }
}

impl VulkanResource for OwnedDevice {
    type Raw = Arc<vulkanalia::Device>;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.handle
    }
}

impl Drop for OwnedDevice {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.handle.destroy_device(None);
        }
    }
}
