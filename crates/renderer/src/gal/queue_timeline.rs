use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;
use crate::gal::semaphore_resource::SemaphoreResource;

pub struct QueueTimeline {
    device: Arc<DeviceResource>,
    semaphore: Arc<SemaphoreResource>,
    handle: vk::Semaphore,
}

impl QueueTimeline {
    pub(crate) fn new(device: Arc<DeviceResource>, semaphore: Arc<SemaphoreResource>) -> Self {
        let handle = unsafe { semaphore.handle() };
        Self {
            device,
            semaphore,
            handle,
        }
    }

    pub(crate) fn poll(&self) -> Result<TimelineValue> {
        use vulkanalia::prelude::v1_2::*;

        let value = unsafe { self.device.handle().get_semaphore_counter_value(self.handle)? };
        Ok(TimelineValue::new(value))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TimelineValue(u64);

impl TimelineValue {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }
}

impl From<TimelineValue> for u64 {
    fn from(value: TimelineValue) -> Self {
        value.0
    }
}
