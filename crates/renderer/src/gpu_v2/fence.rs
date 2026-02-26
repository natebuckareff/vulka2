use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::{OwnedFence, ResourceArena, VulkanHandle};

#[derive(Clone)]
pub struct Fence {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    fence: VulkanHandle<vk::Fence>,
}

impl Fence {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        arena: &ResourceArena,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        let fence = OwnedFence::new(device.clone(), &info)?;
        let fence = arena.add(fence)?;
        Ok(Self { device, fence })
    }

    pub fn handle(&self) -> &VulkanHandle<vk::Fence> {
        &self.fence
    }

    pub fn wait(&self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;
        // TODO: handle timeouts??
        unsafe {
            let fences = &[*self.fence.raw()];
            self.device.raw().wait_for_fences(fences, true, u64::MAX)?;
        }
        Ok(())
    }

    pub fn reset(&self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            let fences = &[*self.fence.raw()];
            self.device.raw().reset_fences(fences)?;
        }
        Ok(())
    }
}
