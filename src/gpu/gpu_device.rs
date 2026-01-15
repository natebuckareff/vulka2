use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, anyhow};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;

use crate::gpu::{GpuPhysicalDevice, GpuQueue, GpuQueueFamilyIndex, GpuQueueFamilyIntent};

pub struct GpuDeviceBuilder {
    physical_device: Arc<GpuPhysicalDevice>,
    enabled_extensions: Option<Vec<vk::ExtensionName>>,
    queues: HashMap<GpuQueueFamilyIndex, QueueRequest>,
}

struct QueueRequest {
    priorities: Vec<f32>,
    intents: Vec<GpuQueueFamilyIntent>,
}

impl GpuDeviceBuilder {
    pub fn new(physical_device: Arc<GpuPhysicalDevice>) -> Self {
        Self {
            physical_device,
            enabled_extensions: None,
            queues: HashMap::new(),
        }
    }

    pub fn enabled_extensions(mut self, extensions: Vec<vk::ExtensionName>) -> Self {
        self.enabled_extensions = Some(extensions);
        self
    }

    pub fn create_queue(mut self, intent: GpuQueueFamilyIntent) -> Result<Self> {
        let queue_family_index = self
            .physical_device
            .get_queue_family(intent)
            .ok_or_else(|| anyhow!("queue family not found for usage: {:?}", intent))?;

        use std::collections::hash_map::Entry;
        match self.queues.entry(queue_family_index) {
            Entry::Occupied(mut entry) => {
                let intents = &mut entry.get_mut().intents;
                if !intents.contains(&intent) {
                    intents.push(intent);
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(QueueRequest {
                    priorities: vec![1.0],
                    intents: vec![intent],
                });
            }
        }

        Ok(self)
    }

    pub fn build(self) -> Result<Arc<GpuDevice>> {
        if self.queues.is_empty() {
            return Err(anyhow!("no queues requested"));
        }

        let mut queue_items = self
            .queues
            .into_iter()
            .map(|(index, request)| (index, request))
            .collect::<Vec<_>>();

        queue_items.sort_by(|a, b| a.0.cmp(&b.0));

        let extension_names = self
            .enabled_extensions
            .unwrap_or_default()
            .iter()
            .map(|ext| ext.as_ptr())
            .collect::<Vec<_>>();

        let queue_create_infos = queue_items
            .iter()
            .map(|(index, request)| {
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(*index)
                    .queue_priorities(&request.priorities)
                    .build()
            })
            .collect::<Vec<_>>();

        let device_info = vk::DeviceCreateInfo::builder()
            .enabled_extension_names(&extension_names)
            .queue_create_infos(&queue_create_infos);

        let instance = self.physical_device.get_vk_instance();
        let device = unsafe {
            instance.create_device(
                self.physical_device.get_vk_physical_device(),
                &device_info,
                None,
            )
        }
        .map_err(|err| anyhow!("failed to create Vulkan device: {err}"))?;

        let mut created_queues = Vec::with_capacity(queue_items.len());
        for (index, request) in queue_items {
            let queue = unsafe { device.get_device_queue(index, 0) };
            created_queues.push(GpuQueue::new(request.intents, index, queue));
        }

        Ok(Arc::new(GpuDevice::new(
            device,
            self.physical_device,
            created_queues,
        )))
    }
}

pub struct GpuDevice {
    device: Device,
    physical_device: Arc<GpuPhysicalDevice>,
    queues: Vec<GpuQueue>,
}

impl GpuDevice {
    fn new(
        device: Device,
        physical_device: Arc<GpuPhysicalDevice>,
        queues: Vec<GpuQueue>,
    ) -> Self {
        Self {
            device,
            physical_device,
            queues,
        }
    }

    pub fn physical_device(&self) -> &Arc<GpuPhysicalDevice> {
        &self.physical_device
    }

    pub fn get_vk_device(&self) -> &Device {
        &self.device
    }

    pub fn get_queue(&self, intent: GpuQueueFamilyIntent) -> Option<&GpuQueue> {
        self.queues.iter().find(|queue| queue.supports(intent))
    }
}

impl Drop for GpuDevice {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
        }
    }
}
