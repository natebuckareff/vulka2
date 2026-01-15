use std::sync::Arc;

use vulkano::device::Queue;

use crate::gpu::{GpuQueueFamilyIndex, GpuQueueFamilyIntent};

#[derive(Clone)]
pub struct GpuQueue {
    intents: Vec<GpuQueueFamilyIntent>,
    family_index: GpuQueueFamilyIndex,
    queue: Arc<Queue>,
}

impl GpuQueue {
    pub fn new(
        intents: Vec<GpuQueueFamilyIntent>,
        family_index: GpuQueueFamilyIndex,
        queue: Arc<Queue>,
    ) -> Self {
        Self {
            intents,
            family_index,
            queue,
        }
    }

    pub fn supports(&self, intent: GpuQueueFamilyIntent) -> bool {
        self.intents.contains(&intent)
    }

    pub fn family_index(&self) -> GpuQueueFamilyIndex {
        self.family_index
    }

    pub fn get_vk_queue(&self) -> &Arc<Queue> {
        &self.queue
    }
}
