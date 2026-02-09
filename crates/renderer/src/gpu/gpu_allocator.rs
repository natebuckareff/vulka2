use std::sync::Arc;

use anyhow::Result;
use vulkanalia::prelude::v1_3::*;
use vulkanalia_vma::{self as vma};

use super::GpuDevice;

pub struct GpuAllocator {
    allocator: Option<vma::Allocator>,
    device_guard: Arc<GpuDevice>,
}

impl GpuAllocator {
    pub fn new(
        instance: &Instance,
        device: Arc<GpuDevice>,
        physical_device: vk::PhysicalDevice,
        flags: vma::AllocatorCreateFlags,
    ) -> Result<Arc<Self>> {
        let mut options = vma::AllocatorOptions::new(instance, device.get_vk_device(), physical_device);
        options.flags = flags;
        let allocator = unsafe { vma::Allocator::new(&options) }?;
        Ok(Arc::new(Self {
            allocator: Some(allocator),
            device_guard: device,
        }))
    }

    pub fn raw(&self) -> &vma::Allocator {
        self.allocator
            .as_ref()
            .expect("GpuAllocator must be valid until drop")
    }

    pub fn device_arc(&self) -> Arc<GpuDevice> {
        self.device_guard.clone()
    }
}

impl Drop for GpuAllocator {
    fn drop(&mut self) {
        let _ = self.allocator.take();
    }
}
