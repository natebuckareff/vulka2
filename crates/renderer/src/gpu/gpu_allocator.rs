use std::sync::Arc;

use anyhow::Result;
use vulkanalia_vma as vma;

use crate::gpu::Device;

pub struct GpuAllocator {
    device: Arc<Device>,
    allocator: vma::Allocator,
}

impl GpuAllocator {
    pub fn new(device: Arc<Device>) -> Result<Arc<Self>> {
        let allocator = {
            let instance = unsafe { device.engine().instance().raw() };
            let physical_device = device.info().physical_device;
            let device = unsafe { device.handle().raw() };
            let options = vma::AllocatorOptions::new(instance, device, physical_device);
            unsafe { vma::Allocator::new(&options)? }
        };
        Ok(Arc::new(Self { device, allocator }))
    }

    pub fn device(&self) -> &Arc<Device> {
        &self.device
    }

    pub(crate) unsafe fn raw(&self) -> &vma::Allocator {
        &self.allocator
    }
}
