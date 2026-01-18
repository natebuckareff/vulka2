use std::sync::Arc;

use anyhow::{Context, Result};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;
use vulkanalia::window::{create_surface, get_required_instance_extensions};
use winit::window::Window;

use crate::gpu::GpuInstance;

pub struct GpuSurface {
    instance: Arc<GpuInstance>,
    surface: vk::SurfaceKHR,
}

impl GpuSurface {
    pub fn required_instance_extensions(window: &Window) -> &'static [&'static vk::ExtensionName] {
        get_required_instance_extensions(window)
    }

    pub fn new(instance: Arc<GpuInstance>, window: &Window) -> Result<Arc<Self>> {
        let surface = unsafe {
            create_surface(instance.get_vk_instance(), window, window)
                .context("failed to create window surface")?
        };

        Ok(Arc::new(Self { instance, surface }))
    }

    pub fn surface(&self) -> vk::SurfaceKHR {
        self.surface
    }
}

impl Drop for GpuSurface {
    fn drop(&mut self) {
        unsafe {
            self.instance
                .get_vk_instance()
                .destroy_surface_khr(self.surface, None);
        }
    }
}
