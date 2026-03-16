use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedSwapchain {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    handle: vk::SwapchainKHR,
}

impl OwnedSwapchain {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::SwapchainCreateInfoKHRBuilder,
    ) -> Result<Self> {
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
        let handle = unsafe { device.raw().create_swapchain_khr(info, None)? };
        Ok(Self { device, handle })
    }
}

impl VulkanResource for OwnedSwapchain {
    type Raw = vk::SwapchainKHR;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.handle
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
            self.device.raw().destroy_swapchain_khr(self.handle, None);
        }
    }
}
