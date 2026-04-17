use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::Engine;
use crate::gal::device_allocator::DeviceAllocator;
use crate::gal::device_timeline::DeviceTimeline;
use crate::gal::engine;

pub struct Device {
    engine: Arc<Engine>,
    physical_device: vk::PhysicalDevice,
    resource: Arc<DeviceResource>,
    timeline: DeviceTimeline,
    allocator: DeviceAllocator,
}

impl Device {
    pub(crate) fn new(
        engine: Arc<Engine>,
        physical_device: vk::PhysicalDevice,
        resource: Arc<DeviceResource>,
        timeline: DeviceTimeline,
    ) -> Result<Self> {
        let allocator = DeviceAllocator::new(&engine, &resource, physical_device)?;
        Ok(Self {
            engine,
            physical_device,
            resource,
            timeline,
            allocator,
        })
    }

    pub(crate) fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    pub(crate) fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub(crate) fn resource(&self) -> &Arc<DeviceResource> {
        &self.resource
    }

    pub(crate) fn timeline(&self) -> &DeviceTimeline {
        &self.timeline
    }

    pub(crate) fn allocator(&self) -> &DeviceAllocator {
        &self.allocator
    }

    pub fn wait_idle(&self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;

        unsafe {
            self.resource.handle().device_wait_idle()?;
        }

        Ok(())
    }
}

pub struct DeviceResource {
    engine: Arc<Engine>,
    handle: vulkanalia::Device,
}

impl DeviceResource {
    pub(crate) fn new(engine: Arc<Engine>, handle: vulkanalia::Device) -> Self {
        Self { engine, handle }
    }

    pub(crate) unsafe fn handle(&self) -> &vulkanalia::Device {
        &self.handle
    }
}

impl Drop for DeviceResource {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;

        unsafe {
            self.handle.destroy_device(None);
        }
    }
}
