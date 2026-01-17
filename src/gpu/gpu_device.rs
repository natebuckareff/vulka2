use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;

use crate::gpu::{
    GpuDeviceFeatures, GpuExtensions, GpuPhysicalDevice, GpuQueue, GpuQueueFamilyIndex,
    GpuQueueFamilyIntent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueSelectionPolicy {
    PreferShared,
    PreferSeparate,
    RequireShared,
    RequireSeparate,
}

impl Default for QueueSelectionPolicy {
    fn default() -> Self {
        Self::PreferShared
    }
}

pub struct GpuDeviceBuilder {
    physical_device: Arc<GpuPhysicalDevice>,
    enabled_extensions: Arc<GpuExtensions>,
    features: GpuDeviceFeatures,
    requested_intents: HashSet<GpuQueueFamilyIntent>,
    queue_selection_policy: QueueSelectionPolicy,
}

struct QueueRequest {
    priorities: Vec<f32>,
    intents: Vec<GpuQueueFamilyIntent>,
}

impl GpuDeviceBuilder {
    pub fn new(physical_device: Arc<GpuPhysicalDevice>) -> Self {
        Self {
            physical_device,
            enabled_extensions: GpuExtensions::empty(),
            features: GpuDeviceFeatures::default(),
            requested_intents: HashSet::new(),
            queue_selection_policy: QueueSelectionPolicy::default(),
        }
    }

    pub fn enabled_extensions(mut self, extensions: Arc<GpuExtensions>) -> Self {
        self.enabled_extensions = extensions;
        self
    }

    pub fn features(mut self, features: GpuDeviceFeatures) -> Self {
        self.features = features;
        self
    }

    pub fn create_queue(mut self, intent: GpuQueueFamilyIntent) -> Result<Self> {
        self.requested_intents.insert(intent);

        Ok(self)
    }

    pub fn queue_selection_policy(mut self, policy: QueueSelectionPolicy) -> Self {
        self.queue_selection_policy = policy;
        self
    }

    pub fn build(self) -> Result<Arc<GpuDevice>> {
        if self.requested_intents.is_empty() {
            return Err(anyhow!("no queues requested"));
        }

        let instance = self.physical_device.get_vk_instance();
        let physical_device = self.physical_device.get_vk_physical_device();
        let extension_support = self
            .enabled_extensions
            .support_for(instance, physical_device)?;
        let missing = extension_support.missing_extension_names();
        if !missing.is_empty() {
            let properties = unsafe { instance.get_physical_device_properties(physical_device) };
            let device_name = unsafe { std::ffi::CStr::from_ptr(properties.device_name.as_ptr()) }
                .to_string_lossy();
            return Err(anyhow!(
                "device {device_name} missing extensions: {}",
                missing.join(", ")
            ));
        }

        let grouped = self.physical_device.get_grouped_queue_families();
        let mut queue_requests: HashMap<GpuQueueFamilyIndex, QueueRequest> = HashMap::new();

        let mut insert_intent = |index: GpuQueueFamilyIndex, intent: GpuQueueFamilyIntent| {
            queue_requests
                .entry(index)
                .and_modify(|request| {
                    if !request.intents.contains(&intent) {
                        request.intents.push(intent);
                    }
                })
                .or_insert_with(|| QueueRequest {
                    priorities: vec![1.0],
                    intents: vec![intent],
                });
        };

        let pick_family = |intent: GpuQueueFamilyIntent| -> Result<GpuQueueFamilyIndex> {
            grouped
                .get(&intent)
                .and_then(|families| families.iter().min().copied())
                .ok_or_else(|| anyhow!("queue family not found for usage: {:?}", intent))
        };

        let needs_graphics = self
            .requested_intents
            .contains(&GpuQueueFamilyIntent::Graphics);
        let needs_present = self
            .requested_intents
            .contains(&GpuQueueFamilyIntent::Present);

        if needs_graphics && needs_present {
            let graphics_families =
                grouped
                    .get(&GpuQueueFamilyIntent::Graphics)
                    .ok_or_else(|| {
                        anyhow!(
                            "queue family not found for usage: {:?}",
                            GpuQueueFamilyIntent::Graphics
                        )
                    })?;
            let present_families =
                grouped.get(&GpuQueueFamilyIntent::Present).ok_or_else(|| {
                    anyhow!(
                        "queue family not found for usage: {:?}",
                        GpuQueueFamilyIntent::Present
                    )
                })?;

            let shared = graphics_families
                .iter()
                .filter(|index| present_families.contains(index))
                .min()
                .copied();
            let distinct = {
                let mut graphics_sorted = graphics_families.iter().copied().collect::<Vec<_>>();
                let mut present_sorted = present_families.iter().copied().collect::<Vec<_>>();
                graphics_sorted.sort_unstable();
                present_sorted.sort_unstable();

                if graphics_sorted.is_empty() || present_sorted.is_empty() {
                    None
                } else {
                    let graphics_min = graphics_sorted[0];
                    let present_min = present_sorted[0];
                    if graphics_min != present_min {
                        Some((graphics_min, present_min))
                    } else if let Some(present_alt) = present_sorted
                        .iter()
                        .copied()
                        .find(|index| *index != graphics_min)
                    {
                        Some((graphics_min, present_alt))
                    } else if let Some(graphics_alt) = graphics_sorted
                        .iter()
                        .copied()
                        .find(|index| *index != present_min)
                    {
                        Some((graphics_alt, present_min))
                    } else {
                        None
                    }
                }
            };

            match self.queue_selection_policy {
                QueueSelectionPolicy::PreferShared => {
                    if let Some(index) = shared {
                        insert_intent(index, GpuQueueFamilyIntent::Graphics);
                        insert_intent(index, GpuQueueFamilyIntent::Present);
                    } else {
                        let graphics_index = pick_family(GpuQueueFamilyIntent::Graphics)?;
                        let present_index = pick_family(GpuQueueFamilyIntent::Present)?;
                        insert_intent(graphics_index, GpuQueueFamilyIntent::Graphics);
                        insert_intent(present_index, GpuQueueFamilyIntent::Present);
                    }
                }
                QueueSelectionPolicy::PreferSeparate => {
                    if let Some((graphics_index, present_index)) = distinct {
                        insert_intent(graphics_index, GpuQueueFamilyIntent::Graphics);
                        insert_intent(present_index, GpuQueueFamilyIntent::Present);
                    } else if let Some(index) = shared {
                        insert_intent(index, GpuQueueFamilyIntent::Graphics);
                        insert_intent(index, GpuQueueFamilyIntent::Present);
                    } else {
                        return Err(anyhow!(
                            "queue selection policy could not find graphics/present families"
                        ));
                    }
                }
                QueueSelectionPolicy::RequireShared => {
                    if let Some(index) = shared {
                        insert_intent(index, GpuQueueFamilyIntent::Graphics);
                        insert_intent(index, GpuQueueFamilyIntent::Present);
                    } else {
                        return Err(anyhow!(
                            "queue selection policy requires a shared graphics/present family"
                        ));
                    }
                }
                QueueSelectionPolicy::RequireSeparate => {
                    if let Some((graphics_index, present_index)) = distinct {
                        insert_intent(graphics_index, GpuQueueFamilyIntent::Graphics);
                        insert_intent(present_index, GpuQueueFamilyIntent::Present);
                    } else {
                        return Err(anyhow!(
                            "queue selection policy requires distinct graphics/present families"
                        ));
                    }
                }
            }
        } else {
            for intent in self.requested_intents.iter().copied() {
                let index = pick_family(intent)?;
                insert_intent(index, intent);
            }
        }

        let mut queue_items = queue_requests
            .into_iter()
            .map(|(index, request)| (index, request))
            .collect::<Vec<_>>();

        queue_items.sort_by(|a, b| a.0.cmp(&b.0));

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

        let device = self
            .enabled_extensions
            .with_ptrs(|extension_names| {
                let device_info = vk::DeviceCreateInfo::builder()
                    .enabled_extension_names(extension_names)
                    .queue_create_infos(&queue_create_infos)
                    .push_next(&mut v13_features)
                    .push_next(&mut buffer_device_address_features)
                    .push_next(&mut descriptor_indexing_features);

                unsafe {
                    instance.create_device(
                        self.physical_device.get_vk_physical_device(),
                        &device_info,
                        None,
                    )
                }
            })
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
