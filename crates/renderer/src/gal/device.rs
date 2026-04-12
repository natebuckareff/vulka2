use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::{
    Engine,
    device_allocator::DeviceAllocator,
    queue::{LaneIndex, Queue},
    queue_timeline::QueueTimeline,
};

pub struct Device {
    engine: Arc<Engine>,
    physical_device: vk::PhysicalDevice,
    resource: Arc<DeviceResource>,
    timelines: Vec<QueueTimeline>,
    allocator: DeviceAllocator,
}

impl Device {
    pub(crate) fn new(
        engine: Arc<Engine>,
        physical_device: vk::PhysicalDevice,
        resource: Arc<DeviceResource>,
        queues: &[Queue],
    ) -> Result<Self> {
        let timelines = Self::create_timelines(resource.clone(), queues);
        let allocator = DeviceAllocator::new(&engine, &resource, physical_device)?;
        Ok(Self {
            engine,
            physical_device,
            resource,
            timelines,
            allocator,
        })
    }

    fn create_timelines(device: Arc<DeviceResource>, queues: &[Queue]) -> Vec<QueueTimeline> {
        queues
            .iter()
            .map(|queue| QueueTimeline::new(device.clone(), queue.semaphore().clone()))
            .collect()
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

    pub(crate) fn timeline(&self, index: LaneIndex) -> &QueueTimeline {
        &self.timelines[usize::from(index)]
    }

    pub(crate) fn allocator(&self) -> &DeviceAllocator {
        &self.allocator
    }
}

pub struct DeviceResource {
    handle: vulkanalia::Device,
}

impl DeviceResource {
    pub(crate) fn new(handle: vulkanalia::Device) -> Self {
        Self { handle }
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
