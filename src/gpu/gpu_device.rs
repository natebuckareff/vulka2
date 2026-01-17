use std::collections::HashMap;

use anyhow::{Context, Result};
use vulkanalia::prelude::v1_3::*;

use crate::gpu::{GpuDeviceProfile, GpuInstance};

pub struct GpuQueue {
    request_index: usize,
    priority: f32,
    family_index: u32,
    queue_index: u32,
    queue: vk::Queue,
}

pub struct GpuDevice {
    profile: GpuDeviceProfile,
    device: Device,
    queues: HashMap<usize, GpuQueue>,
}

impl GpuDevice {
    fn new(instance: &GpuInstance, mut profile: GpuDeviceProfile) -> Result<Self> {
        let mut queues_to_create: HashMap<u32, (Vec<usize>, Vec<f32>)> = HashMap::new();
        for selection in profile.iter_queue_families() {
            use std::collections::hash_map::Entry;
            match queues_to_create.entry(selection.queue_family_index) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().0.push(selection.request);
                    entry.get_mut().1.push(selection.priority);
                }
                Entry::Vacant(entry) => {
                    entry.insert((vec![selection.request], vec![selection.priority]));
                }
            }
        }

        let mut queue_create_infos = vec![];
        for (queue_family_index, (_, priorities)) in queues_to_create.iter() {
            let queue_create_info = vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(*queue_family_index)
                .queue_priorities(priorities);
            queue_create_infos.push(queue_create_info);
        }

        let mut features = profile.take_features()?;

        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(profile.extensions().as_ptrs())
            .push_next(features.get_features2());

        let device = unsafe {
            instance
                .get_vk_instance()
                .create_device(profile.physical_device(), &device_create_info, None)
                .context("failed to create device")
        }?;

        let mut queues = HashMap::new();
        for (queue_family_index, (requests, priorities)) in queues_to_create.into_iter() {
            debug_assert_eq!(requests.len(), priorities.len());
            for (i, priority) in priorities.iter().enumerate() {
                let queue_index = i as u32;
                let queue = unsafe { device.get_device_queue(queue_family_index, queue_index) };
                queues.insert(
                    requests[i],
                    GpuQueue {
                        request_index: requests[i],
                        priority: *priority,
                        family_index: queue_family_index,
                        queue_index,
                        queue,
                    },
                );
            }
        }

        Ok(Self {
            profile,
            device,
            queues,
        })
    }

    pub(crate) fn get_vk_device(&self) -> &Device {
        &self.device
    }

    pub fn get_queue(&self, request: usize) -> Option<&GpuQueue> {
        self.queues.get(&request)
    }
}

impl Drop for GpuDevice {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(None);
        }
    }
}
