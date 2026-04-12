use std::sync::Arc;

use anyhow::{Result, anyhow};
use vulkanalia::vk;
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;

use crate::gal::engine::Engine;

pub struct SurfaceResource {
    engine: Arc<Engine>,
    handle: vk::SurfaceKHR,
}

impl SurfaceResource {
    pub(crate) fn new(engine: Arc<Engine>) -> Result<Self> {
        let window = engine
            .window()
            .ok_or_else(|| anyhow!("cannot create a surface for a headless engine"))?;
        let handle = unsafe {
            vulkanalia::window::create_surface(engine.instance(), window.as_ref(), window.as_ref())?
        };

        Ok(Self { engine, handle })
    }

    pub(crate) fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    // XXX TODO: Resource trait?
    pub(crate) unsafe fn handle(&self) -> vk::SurfaceKHR {
        self.handle
    }
}

impl Drop for SurfaceResource {
    fn drop(&mut self) {
        unsafe {
            self.engine
                .instance()
                .destroy_surface_khr(self.handle, None);
        }
    }
}
