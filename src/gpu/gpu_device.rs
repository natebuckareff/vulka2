use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, anyhow};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;

use crate::gpu::{GpuPhysicalDevice, GpuQueue, GpuQueueFamilyIndex, GpuQueueFamilyIntent};

#[derive(Clone)]
pub struct DescriptorIndexingFeatures {
    pub runtime_descriptor_array: bool,
    pub descriptor_binding_partially_bound: bool,
    pub descriptor_binding_variable_descriptor_count: bool,
    pub descriptor_binding_update_unused_while_pending: bool,
    pub shader_sampled_image_array_non_uniform_indexing: bool,
    pub shader_storage_buffer_array_non_uniform_indexing: bool,
    pub shader_storage_image_array_non_uniform_indexing: bool,
}

impl Default for DescriptorIndexingFeatures {
    fn default() -> Self {
        Self {
            runtime_descriptor_array: true,
            descriptor_binding_partially_bound: true,
            descriptor_binding_variable_descriptor_count: true,
            descriptor_binding_update_unused_while_pending: true,
            shader_sampled_image_array_non_uniform_indexing: true,
            shader_storage_buffer_array_non_uniform_indexing: true,
            shader_storage_image_array_non_uniform_indexing: true,
        }
    }
}

#[derive(Clone)]
pub struct GpuDeviceFeatures {
    pub dynamic_rendering: bool,
    pub synchronization2: bool,
    pub buffer_device_address: bool,
    pub descriptor_indexing: DescriptorIndexingFeatures,
}

impl Default for GpuDeviceFeatures {
    fn default() -> Self {
        Self {
            dynamic_rendering: true,
            synchronization2: true,
            buffer_device_address: true,
            descriptor_indexing: DescriptorIndexingFeatures::default(),
        }
    }
}

impl GpuDeviceFeatures {
    pub fn vulkan13_default() -> Self {
        Self::default()
    }
}

pub struct GpuDeviceBuilder {
    physical_device: Arc<GpuPhysicalDevice>,
    enabled_extensions: Option<Vec<vk::ExtensionName>>,
    features: GpuDeviceFeatures,
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
            features: GpuDeviceFeatures::default(),
            queues: HashMap::new(),
        }
    }

    pub fn enabled_extensions(mut self, extensions: Vec<vk::ExtensionName>) -> Self {
        self.enabled_extensions = Some(extensions);
        self
    }

    pub fn features(mut self, features: GpuDeviceFeatures) -> Self {
        self.features = features;
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

        let mut v13_features = vk::PhysicalDeviceVulkan13Features::builder()
            .dynamic_rendering(self.features.dynamic_rendering)
            .synchronization2(self.features.synchronization2);

        let mut buffer_device_address_features =
            vk::PhysicalDeviceBufferDeviceAddressFeatures::builder()
                .buffer_device_address(self.features.buffer_device_address);

        let mut descriptor_indexing_features =
            vk::PhysicalDeviceDescriptorIndexingFeatures::builder()
                .runtime_descriptor_array(
                    self.features.descriptor_indexing.runtime_descriptor_array,
                )
                .descriptor_binding_partially_bound(
                    self.features
                        .descriptor_indexing
                        .descriptor_binding_partially_bound,
                )
                .descriptor_binding_variable_descriptor_count(
                    self.features
                        .descriptor_indexing
                        .descriptor_binding_variable_descriptor_count,
                )
                .descriptor_binding_update_unused_while_pending(
                    self.features
                        .descriptor_indexing
                        .descriptor_binding_update_unused_while_pending,
                )
                .shader_sampled_image_array_non_uniform_indexing(
                    self.features
                        .descriptor_indexing
                        .shader_sampled_image_array_non_uniform_indexing,
                )
                .shader_storage_buffer_array_non_uniform_indexing(
                    self.features
                        .descriptor_indexing
                        .shader_storage_buffer_array_non_uniform_indexing,
                )
                .shader_storage_image_array_non_uniform_indexing(
                    self.features
                        .descriptor_indexing
                        .shader_storage_image_array_non_uniform_indexing,
                );

        let device_info = vk::DeviceCreateInfo::builder()
            .enabled_extension_names(&extension_names)
            .queue_create_infos(&queue_create_infos)
            .push_next(&mut v13_features)
            .push_next(&mut buffer_device_address_features)
            .push_next(&mut descriptor_indexing_features);

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
    fn new(device: Device, physical_device: Arc<GpuPhysicalDevice>, queues: Vec<GpuQueue>) -> Self {
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
