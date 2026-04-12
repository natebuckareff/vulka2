use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::{engine::Engine, surface_resource::SurfaceResource};

pub struct Surface {
    resource: SurfaceResource,
}

// TODO: feels redundant
impl Surface {
    pub fn new(engine: Arc<Engine>) -> Result<Self> {
        let resource = SurfaceResource::new(engine)?;
        Ok(Self { resource })
    }

    pub(crate) unsafe fn handle(&self) -> vk::SurfaceKHR {
        unsafe { self.resource.handle() }
    }

    pub(crate) fn engine(&self) -> &Arc<Engine> {
        self.resource.engine()
    }
}
