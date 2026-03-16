use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;
use winit::window::Window;

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedSurface {
    instance: VulkanHandle<Arc<vulkanalia::Instance>>,
    surface: vk::SurfaceKHR,
}

impl OwnedSurface {
    pub fn new(instance: VulkanHandle<Arc<vulkanalia::Instance>>, window: &Window) -> Result<Self> {
        use vulkanalia::window::create_surface;
        let surface = unsafe { create_surface(instance.raw(), window, window)? };
        Ok(Self { instance, surface })
    }
}

impl VulkanResource for OwnedSurface {
    type Raw = vk::SurfaceKHR;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.surface
    }
}

impl Drop for OwnedSurface {
    fn drop(&mut self) {
        use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;
        unsafe {
            self.instance.raw().destroy_surface_khr(self.surface, None);
        }
    }
}
