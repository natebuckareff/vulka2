use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, anyhow};
use vulkano::device::{Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo};

use crate::gpu::{GpuPhysicalDevice, GpuQueueFamilyIndex, GpuQueueFamilyIntent};

pub struct GpuDeviceBuilder {
    physical_device: Arc<GpuPhysicalDevice>,
    enabled_extensions: Option<DeviceExtensions>,
    queues: HashMap<GpuQueueFamilyIndex, (QueueCreateInfo, Vec<GpuQueueFamilyIntent>)>,
}

impl GpuDeviceBuilder {
    pub fn new(physical_device: Arc<GpuPhysicalDevice>) -> Self {
        Self {
            physical_device,
            enabled_extensions: None,
            queues: HashMap::new(),
        }
    }

    pub fn enabled_extensions(mut self, extensions: DeviceExtensions) -> Self {
        self.enabled_extensions = Some(extensions);
        self
    }

    pub fn create_queue(mut self, intent: GpuQueueFamilyIntent) -> Result<Self> {
        let queue_family_index = self
            .physical_device
            .get_queue_family(intent)
            .ok_or_else(|| anyhow!("queue family not found for usage: {:?}", intent))?;

        let info = QueueCreateInfo {
            queue_family_index,
            ..Default::default()
        };

        use std::collections::hash_map::Entry;
        match self.queues.entry(queue_family_index) {
            Entry::Occupied(mut entry) => {
                let (_, intents) = entry.get_mut();
                if !intents.contains(&intent) {
                    intents.push(intent);
                }
            }
            Entry::Vacant(entry) => {
                entry.insert((info, vec![intent]));
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
            .map(|(index, (info, intents))| (index, info, intents))
            .collect::<Vec<_>>();

        queue_items.sort_by(|a, b| a.0.cmp(&b.0));

        let (device, mut queues) = Device::new(
            self.physical_device.get_vk_physical_device().clone(),
            DeviceCreateInfo {
                enabled_extensions: self.enabled_extensions.unwrap_or_default(),
                queue_create_infos: queue_items
                    .iter()
                    .map(|(_, info, _)| info.clone())
                    .collect(),
                ..Default::default()
            },
        )?;

        let mut created_queues = vec![];
        for (index, _, intents) in queue_items {
            let queue = queues
                .next()
                .ok_or_else(|| anyhow!("no queue found for family index: {:?}", index))?;
            created_queues.push((index, intents, queue));
        }

        Ok(Arc::new(GpuDevice::new(
            device,
            self.physical_device,
            created_queues,
        )))
    }
}

pub struct GpuDevice {
    device: Arc<Device>,
    physical_device: Arc<GpuPhysicalDevice>,
    queues: Vec<(GpuQueueFamilyIndex, Vec<GpuQueueFamilyIntent>, Arc<Queue>)>,
}

impl GpuDevice {
    fn new(
        device: Arc<Device>,
        physical_device: Arc<GpuPhysicalDevice>,
        queues: Vec<(GpuQueueFamilyIndex, Vec<GpuQueueFamilyIntent>, Arc<Queue>)>,
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

    pub fn get_vk_device(&self) -> &Arc<Device> {
        &self.device
    }

    // TODO: until we add GpuQueue
    pub fn get_first_vk_queue(&self, intent: GpuQueueFamilyIntent) -> Option<&Arc<Queue>> {
        self.queues.iter().find_map(|(_, intents, queue)| {
            if intents.contains(&intent) {
                Some(queue)
            } else {
                None
            }
        })
    }
}
