use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;

pub struct SwapchainResource {
    device: Arc<DeviceResource>,
    handle: vk::SwapchainKHR,
}

impl SwapchainResource {
    pub(crate) fn new(
        device: Arc<DeviceResource>,
        info: &vk::SwapchainCreateInfoKHRBuilder,
    ) -> Result<Self> {
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
        let handle = unsafe { device.handle().create_swapchain_khr(info, None)? };
        Ok(Self { device, handle })
    }

    pub(crate) unsafe fn handle(&self) -> vk::SwapchainKHR {
        self.handle
    }
}

impl Drop for SwapchainResource {
    fn drop(&mut self) {
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
        unsafe {
            self.device
                .handle()
                .destroy_swapchain_khr(self.handle, None);
        }
    }
}
