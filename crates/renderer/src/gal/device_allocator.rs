use anyhow::Result;
use vulkanalia::vk;
use vulkanalia_vma as vma;

use crate::gal::{Engine, device::DeviceResource};

pub struct DeviceAllocator {
    allocator: vma::Allocator,
}

impl DeviceAllocator {
    pub(crate) fn new(
        engine: &Engine,
        device: &DeviceResource,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self> {
        let allocator = {
            let instance = unsafe { engine.instance() };
            let device = unsafe { device.handle() };
            let options = vma::AllocatorOptions::new(instance, device, physical_device);
            unsafe { vma::Allocator::new(&options)? }
        };
        Ok(Self { allocator })
    }

    // TODO: rename to raw everywhere for the unsafe handle pattern?
    pub(crate) unsafe fn handle(&self) -> &vma::Allocator {
        &self.allocator
    }
}
