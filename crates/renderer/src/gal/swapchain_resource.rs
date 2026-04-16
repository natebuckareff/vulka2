use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;

pub struct SwapchainResource {
    device: Arc<DeviceResource>,
    handle: vk::SwapchainKHR,
    format: vk::Format,
    extent: vk::Extent2D,
}

impl SwapchainResource {
    pub(crate) fn new(
        device: Arc<DeviceResource>,
        format: vk::Format,
        extent: vk::Extent2D,
        info: &vk::SwapchainCreateInfoKHRBuilder,
    ) -> Result<Self> {
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
        let handle = unsafe { device.handle().create_swapchain_khr(info, None)? };
        Ok(Self {
            device,
            handle,
            format,
            extent,
        })
    }

    pub(crate) fn device(&self) -> &Arc<DeviceResource> {
        &self.device
    }

    pub(crate) unsafe fn handle(&self) -> vk::SwapchainKHR {
        self.handle
    }

    pub(crate) fn format(&self) -> vk::Format {
        self.format
    }

    pub(crate) fn extent(&self) -> vk::Extent2D {
        self.extent
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
