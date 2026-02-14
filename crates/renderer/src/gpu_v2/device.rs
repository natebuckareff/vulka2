use std::sync::Arc;

use anyhow::Result;

use crate::gpu_v2::{DeviceInfo, Engine, QueueFamilyId, QueueGroupBuilder};

pub struct DeviceBuilder {
    engine: Arc<Engine>,
    info: DeviceInfo,
    families: Vec<(QueueFamilyId, u32)>,
}

impl DeviceBuilder {
    pub(crate) fn new(engine: Arc<Engine>, info: DeviceInfo) -> Self {
        Self {
            engine,
            info,
            families: Vec::new(),
        }
    }

    pub fn queue_group(&'_ mut self) -> QueueGroupBuilder<'_> {
        QueueGroupBuilder::new(self)
    }

    pub fn build(self) -> Result<Arc<Device>> {
        Ok(Device::new()?)
    }
}

pub struct Device {
    //
}

impl Device {
    pub(crate) fn new() -> Result<Arc<Self>> {
        let device = Self {};
        Ok(Arc::new(device))
    }
}
