use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;
use vulkanalia_vma::{self as vma, Alloc};

use super::{GpuAllocator, GpuDevice};

#[derive(Clone)]
pub struct GpuBuffer {
    inner: Arc<GpuBufferInner>,
}

#[derive(Clone)]
pub struct GpuBufferView {
    buffer: GpuBuffer,
    offset: vk::DeviceSize,
    size: vk::DeviceSize,
}

struct GpuBufferInner {
    allocator: Arc<GpuAllocator>,
    buffer: vk::Buffer,
    allocation: vma::Allocation,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
}

impl GpuBuffer {
    pub fn create(
        allocator: Arc<GpuAllocator>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        allocation_options: &vma::AllocationOptions,
    ) -> Result<Self> {
        let buffer_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (buffer, allocation) = unsafe { allocator.raw().create_buffer(buffer_info, allocation_options) }
            .map_err(|err| anyhow!(err))
            .context("failed to create VMA buffer")?;

        Ok(Self {
            inner: Arc::new(GpuBufferInner {
                allocator,
                buffer,
                allocation,
                size,
                usage,
            }),
        })
    }

    pub fn handle(&self) -> vk::Buffer {
        self.inner.buffer
    }

    pub fn size(&self) -> vk::DeviceSize {
        self.inner.size
    }

    pub fn usage(&self) -> vk::BufferUsageFlags {
        self.inner.usage
    }

    pub fn whole_view(&self) -> GpuBufferView {
        GpuBufferView {
            buffer: self.clone(),
            offset: 0,
            size: self.inner.size,
        }
    }

    pub fn view(&self, offset: vk::DeviceSize, size: vk::DeviceSize) -> Result<GpuBufferView> {
        Self::checked_subrange(self.inner.size, offset, size, "buffer view")?;
        Ok(GpuBufferView {
            buffer: self.clone(),
            offset,
            size,
        })
    }

    pub fn device_address(&self, device: &GpuDevice) -> Result<vk::DeviceAddress> {
        if !self
            .inner
            .usage
            .contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS)
        {
            return Err(anyhow!(
                "buffer was not created with SHADER_DEVICE_ADDRESS usage"
            ));
        }

        let info = vk::BufferDeviceAddressInfo::builder().buffer(self.inner.buffer);
        let address = unsafe { device.get_vk_device().get_buffer_device_address(&info) };
        Ok(address)
    }

    pub(crate) fn allocator(&self) -> &vma::Allocator {
        self.inner.allocator.raw()
    }

    pub(crate) fn allocation(&self) -> vma::Allocation {
        self.inner.allocation
    }

    pub(crate) fn checked_subrange(
        total_size: vk::DeviceSize,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
        label: &str,
    ) -> Result<()> {
        let end = offset
            .checked_add(size)
            .ok_or_else(|| anyhow!("{label} range overflows"))?;
        if end > total_size {
            return Err(anyhow!(
                "{label} out of bounds: start={} end={} size={}",
                offset,
                end,
                total_size
            ));
        }
        Ok(())
    }
}

impl GpuBufferView {
    pub fn handle(&self) -> vk::Buffer {
        self.buffer.handle()
    }

    pub fn offset(&self) -> vk::DeviceSize {
        self.offset
    }

    pub fn size(&self) -> vk::DeviceSize {
        self.size
    }

    pub fn gpu_buffer(&self) -> &GpuBuffer {
        &self.buffer
    }

    pub fn subview(&self, offset: vk::DeviceSize, size: vk::DeviceSize) -> Result<Self> {
        GpuBuffer::checked_subrange(self.size, offset, size, "buffer subview")?;
        let absolute_offset = self
            .offset
            .checked_add(offset)
            .ok_or_else(|| anyhow!("buffer subview offset overflows"))?;
        Ok(Self {
            buffer: self.buffer.clone(),
            offset: absolute_offset,
            size,
        })
    }

    pub fn device_address(&self, device: &GpuDevice) -> Result<vk::DeviceAddress> {
        let base = self.buffer.device_address(device)?;
        base.checked_add(self.offset)
            .ok_or_else(|| anyhow!("buffer view device address overflow"))
    }
}

impl Drop for GpuBufferInner {
    fn drop(&mut self) {
        unsafe {
            self.allocator
                .raw()
                .destroy_buffer(self.buffer, self.allocation);
        }
    }
}
