use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::u64;

use anyhow::Result;

use crate::gal::queue::LaneIndex;
use crate::gal::semaphore_resource::SemaphoreResource;
use crate::gal::usage_token::LaneMask;

pub struct DeviceTimeline {
    lanes: Vec<AtomicU64>,
    semaphores: Vec<Arc<SemaphoreResource>>,
}

impl DeviceTimeline {
    pub(crate) fn new(queue_semaphores: &[Arc<SemaphoreResource>]) -> Self {
        let mut lanes = Vec::with_capacity(queue_semaphores.len());
        let mut semaphores = Vec::with_capacity(queue_semaphores.len());
        lanes.extend(semaphores.iter().map(|_| AtomicU64::new(u64::MAX)));
        semaphores.extend(queue_semaphores.iter().cloned());
        Self { lanes, semaphores }
    }

    // called by Queue when it submits the _last_ submission for a frame
    pub(crate) fn update(&self, frame: u32, lane: LaneIndex, submissions: u32) {
        assert!(frame != u32::MAX || submissions != u32::MAX);
        let value = pack(frame, submissions);
        self.lanes[usize::from(lane)].store(value, Ordering::Relaxed);
    }

    pub(crate) fn poll(&self, frame: u32, lane: LaneIndex) -> Result<bool> {
        use vulkanalia::prelude::v1_2::*;
        let Some((last_frame, submissions)) = self.get(lane) else {
            return Ok(false);
        };
        if last_frame < frame {
            return Ok(false);
        }
        let semaphore = &self.semaphores[usize::from(lane)];
        let handle = unsafe { semaphore.handle() };
        let value = unsafe {
            semaphore
                .device()
                .handle()
                .get_semaphore_counter_value(handle)?
        };
        Ok(submissions as u64 <= value)
    }

    pub(crate) fn wait(&self, frame: u32, lane: LaneIndex) -> Result<bool> {
        use vulkanalia::prelude::v1_2::*;

        let Some((last_frame, submissions)) = self.get(lane) else {
            return Ok(false);
        };

        if last_frame < frame {
            return Ok(false);
        }

        let semaphore = &self.semaphores[usize::from(lane)];
        let handle = unsafe { semaphore.handle() };
        let semaphores = [handle];
        let values = [u64::from(submissions)];
        let info = vulkanalia::vk::SemaphoreWaitInfo::builder()
            .semaphores(&semaphores)
            .values(&values);

        unsafe {
            semaphore
                .device()
                .handle()
                .wait_semaphores(&info, u64::MAX)?;
        }

        Ok(true)
    }

    pub(crate) fn wait_many(&self, frame: u32, mask: LaneMask) -> Result<bool> {
        use vulkanalia::prelude::v1_2::*;

        let mut semaphores = Vec::new();
        let mut values = Vec::new();

        for lane in mask.iter() {
            let Some((last_frame, submissions)) = self.get(lane) else {
                return Ok(false);
            };
            if last_frame < frame {
                return Ok(false);
            }

            let semaphore = &self.semaphores[usize::from(lane)];
            semaphores.push(unsafe { semaphore.handle() });
            values.push(u64::from(submissions));
        }

        if semaphores.is_empty() {
            return Ok(true);
        }

        let semaphore = &self.semaphores[0];
        let info = vulkanalia::vk::SemaphoreWaitInfo::builder()
            .semaphores(&semaphores)
            .values(&values);

        unsafe {
            semaphore
                .device()
                .handle()
                .wait_semaphores(&info, u64::MAX)?;
        }

        Ok(true)
    }

    fn get(&self, lane: LaneIndex) -> Option<(u32, u32)> {
        let value = self.lanes[usize::from(lane)].load(Ordering::Relaxed);
        if value == u64::MAX {
            return None;
        }
        Some(unpack(value))
    }
}

fn pack(frame: u32, value: u32) -> u64 {
    (u64::from(frame) << 32) | u64::from(value)
}

fn unpack(value: u64) -> (u32, u32) {
    let frame = (value >> 32) as u32;
    let submissions = value as u32;
    (frame, submissions)
}
