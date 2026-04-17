use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::Device;
use crate::gal::Surface;

pub struct SwapchainResource {
    device: Arc<Device>,
    surface: Arc<Surface>, // TODO: should probably be owned?
    handle: vk::SwapchainKHR,
    format: vk::Format,
    extent: vk::Extent2D,
}

impl SwapchainResource {
    pub(crate) fn new(
        device: Arc<Device>,
        surface: Arc<Surface>,
        format: vk::Format,
        extent: vk::Extent2D,
        info: &vk::SwapchainCreateInfoKHRBuilder,
    ) -> Result<Self> {
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
        let handle = unsafe {
            device
                .resource()
                .handle()
                .create_swapchain_khr(info, None)?
        };
        Ok(Self {
            device,
            surface,
            handle,
            format,
            extent,
        })
    }

    pub(crate) fn device(&self) -> &Arc<Device> {
        &self.device
    }

    pub(crate) fn surface(&self) -> &Arc<Surface> {
        &self.surface
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
                .resource()
                .handle()
                .destroy_swapchain_khr(self.handle, None);
        }
    }
}
