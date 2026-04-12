use std::sync::Arc;

use vulkanalia::vk;

use crate::gal::{
    Engine,
    queue::{LaneIndex, Queue},
    queue_timeline::QueueTimeline,
};

pub struct Device {
    engine: Arc<Engine>,
    physical_device: vk::PhysicalDevice,
    resource: Arc<DeviceResource>,
    timelines: Vec<QueueTimeline>,
}

impl Device {
    pub(crate) fn new(
        engine: Arc<Engine>,
        physical_device: vk::PhysicalDevice,
        resource: Arc<DeviceResource>,
        queues: &[Queue],
    ) -> Self {
        let timelines = queues
            .iter()
            .map(|queue| {
                let semaphore = queue.semaphore();
                QueueTimeline::new(semaphore.clone())
            })
            .collect();
        Self {
            engine,
            physical_device,
            resource,
            timelines,
        }
    }

    pub(crate) fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub(crate) fn resource(&self) -> &DeviceResource {
        self.resource.as_ref()
    }

    pub(crate) fn get_timeline(&self, index: LaneIndex) -> &QueueTimeline {
        &self.timelines[usize::from(index)]
    }
}

pub struct DeviceResource {
    handle: vulkanalia::Device,
}

impl DeviceResource {
    pub(crate) fn new(handle: vulkanalia::Device) -> Self {
        Self { handle }
    }

    pub(crate) fn handle(&self) -> &vulkanalia::Device {
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
