use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::VulkanDevice;

pub struct OwnedSwapchain {
    device: Arc<VulkanDevice>,
    handle: vk::SwapchainKHR,
}

impl OwnedSwapchain {
    pub fn new(
        device: Arc<VulkanDevice>,
        info: &vk::SwapchainCreateInfoKHRBuilder,
    ) -> Result<Self> {
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
        let handle = unsafe { device.create_swapchain_khr(info, None)? };
        Ok(Self { device, handle })
    }
}

impl Deref for OwnedSwapchain {
    type Target = vk::SwapchainKHR;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl Drop for OwnedSwapchain {
    fn drop(&mut self) {
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
        unsafe {
            self.device.destroy_swapchain_khr(self.handle, None);
        }
    }
}
