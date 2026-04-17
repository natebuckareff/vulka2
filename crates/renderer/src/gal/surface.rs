use std::{cell::OnceCell, sync::Arc};

use anyhow::Result;

use crate::gal::Engine;
use crate::gal::{Device, surface_info::SurfaceInfo, surface_resource::SurfaceResource};

pub struct Surface {
    resource: SurfaceResource,
    info: OnceCell<SurfaceInfo>,
}

// TODO: merge with SurfaceResource
impl Surface {
    pub fn new(engine: Arc<Engine>) -> Result<Self> {
        let resource = SurfaceResource::new(engine)?;
        Ok(Self {
            resource,
            info: OnceCell::new(),
        })
    }

    pub(crate) fn resource(&self) -> &SurfaceResource {
        &self.resource
    }

    pub fn info(&self, device: &Device) -> Result<&SurfaceInfo> {
        self.info.get_or_try_init(|| SurfaceInfo::new(device, self))
    }
}
