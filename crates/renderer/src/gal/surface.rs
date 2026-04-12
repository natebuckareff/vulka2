use std::{cell::OnceCell, sync::Arc};

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::{
    Device, engine::Engine, surface_info::SurfaceInfo, surface_resource::SurfaceResource,
};

pub struct Surface {
    resource: SurfaceResource,
    info: OnceCell<SurfaceInfo>,
}

impl Surface {
    pub fn new(engine: Arc<Engine>) -> Result<Self> {
        let resource = SurfaceResource::new(engine)?;
        Ok(Self {
            resource,
            info: OnceCell::new(),
        })
    }

    pub(crate) unsafe fn handle(&self) -> vk::SurfaceKHR {
        unsafe { self.resource.handle() }
    }

    pub(crate) fn engine(&self) -> &Arc<Engine> {
        self.resource.engine()
    }

    pub fn info(&self, device: &Device) -> Result<&SurfaceInfo> {
        self.info.get_or_try_init(|| SurfaceInfo::new(device, self))
    }
}
