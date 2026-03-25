use std::sync::Arc;

use anyhow::Result;
use vulkanalia_vma as vma;

use crate::gpu::{DeviceInfo, Engine, VulkanHandle};

pub struct GpuAllocator {
    allocator: vma::Allocator,
}

impl GpuAllocator {
    pub fn new(
        engine: &Engine,
        device: &VulkanHandle<Arc<vulkanalia::Device>>,
        info: &DeviceInfo,
    ) -> Result<Self> {
        let allocator = {
            let instance = unsafe { engine.instance().raw() };
            let physical_device = info.physical_device;
            let device = unsafe { device.raw() };
            let options = vma::AllocatorOptions::new(instance, device, physical_device);
            unsafe { vma::Allocator::new(&options)? }
        };
        Ok(Self { allocator })
    }

    pub(crate) unsafe fn raw(&self) -> &vma::Allocator {
        &self.allocator
    }
}
