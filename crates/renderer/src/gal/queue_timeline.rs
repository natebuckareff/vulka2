use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::semaphore_resource::SemaphoreResource;

pub struct QueueTimeline {
    semaphore: Arc<SemaphoreResource>,
    handle: vk::Semaphore,
}

impl QueueTimeline {
    pub(crate) fn new(semaphore: Arc<SemaphoreResource>) -> Self {
        let handle = unsafe { semaphore.handle() };
        Self { semaphore, handle }
    }

    pub(crate) fn poll(&self) -> Result<TimelineValue> {
        todo!()
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
