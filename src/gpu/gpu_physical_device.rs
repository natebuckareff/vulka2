use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;

use crate::gpu::GpuInstance;
pub type GpuQueueFamilyIndex = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuQueueFamilyIntent {
    Graphics,
    Present,
}

pub enum GpuPhysicalDeviceCaps {
    Graphics(GpuQueueFamilyIndex),
    Present(GpuQueueFamilyIndex),
}

pub struct GpuPhysicalDevice {
    instance: Arc<GpuInstance>,
    physical_device: vk::PhysicalDevice,
    caps: Vec<GpuPhysicalDeviceCaps>,
}

impl GpuPhysicalDevice {
    pub fn new(
        instance: Arc<GpuInstance>,
        physical_device: vk::PhysicalDevice,
        caps: Vec<GpuPhysicalDeviceCaps>,
    ) -> Self {
        Self {
            instance,
            physical_device,
            caps,
        }
    }

    pub fn get_vk_instance(&self) -> &Instance {
        self.instance.get_vk_instance()
    }

    pub fn instance(&self) -> &Arc<GpuInstance> {
        &self.instance
    }

    pub fn get_vk_physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    // TODO
    #[allow(dead_code)]
    pub fn caps(&self) -> &[GpuPhysicalDeviceCaps] {
        &self.caps
    }

    // TODO
    #[allow(dead_code)]
    pub fn get_queue_families(
        &self,
    ) -> HashMap<GpuQueueFamilyIndex, HashSet<GpuQueueFamilyIntent>> {
        use GpuPhysicalDeviceCaps::*;
        let mut queue_families = HashMap::new();
        for cap in self.caps.iter() {
            match cap {
                Graphics(index) => {
                    queue_families
                        .entry(*index)
                        .or_insert_with(HashSet::new)
                        .insert(GpuQueueFamilyIntent::Graphics);
                }
                Present(index) => {
                    queue_families
                        .entry(*index)
                        .or_insert_with(HashSet::new)
                        .insert(GpuQueueFamilyIntent::Present);
                }
            }
        }
        queue_families
    }

    // TODO
    #[allow(dead_code)]
    pub fn get_grouped_queue_families(
        &self,
    ) -> HashMap<GpuQueueFamilyIntent, HashSet<GpuQueueFamilyIndex>> {
        use GpuPhysicalDeviceCaps::*;
        let mut grouped = HashMap::new();
        for cap in self.caps.iter() {
            match cap {
                Graphics(index) => {
                    grouped
                        .entry(GpuQueueFamilyIntent::Graphics)
                        .or_insert_with(HashSet::new)
                        .insert(*index);
                }
                Present(index) => {
                    grouped
                        .entry(GpuQueueFamilyIntent::Present)
                        .or_insert_with(HashSet::new)
                        .insert(*index);
                }
            }
        }
        grouped
    }

    pub fn get_queue_family(&self, intent: GpuQueueFamilyIntent) -> Option<GpuQueueFamilyIndex> {
        use GpuPhysicalDeviceCaps::*;
        self.caps.iter().find_map(|cap| match (intent, cap) {
            (GpuQueueFamilyIntent::Graphics, Graphics(index)) => Some(*index),
            (GpuQueueFamilyIntent::Present, Present(index)) => Some(*index),
            _ => None,
        })
    }
}
