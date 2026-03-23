use std::{ptr::NonNull, sync::Arc};

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk;
use vulkanalia_vma as vma;

use crate::gpu::GpuAllocator;

pub struct Buffer {
    allocator: Arc<GpuAllocator>,
    buffer: vk::Buffer,
    allocation: vma::Allocation,
    size: u64,
    usage: vk::BufferUsageFlags,
    pointer: Option<NonNull<u8>>,
    is_host_coherent: bool,
}

impl Buffer {
    pub fn new(
        allocator: Arc<GpuAllocator>,
        size: u64,
        usage: vk::BufferUsageFlags,
        flags: vma::AllocationCreateFlags,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        use vulkanalia_vma::Alloc;

        let info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let options = vma::AllocationOptions {
            flags,
            ..Default::default()
        };

        let (buffer, allocation) = unsafe { allocator.raw().create_buffer(info, &options)? };
        let info = unsafe { allocator.raw().get_allocation_info(allocation) };

        let pointer = if flags.contains(vma::AllocationCreateFlags::MAPPED) {
            let ptr = info.pMappedData as *mut u8;
            match NonNull::new(ptr) {
                Some(ptr) => Some(ptr),
                None => None,
            }
        } else {
            None
        };

        let memory_properties = unsafe { allocator.raw().get_memory_properties() };
        let memory_type = memory_properties.memory_types[info.memoryType as usize];
        let is_host_coherent = memory_type
            .property_flags
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);

        Ok(Self {
            allocator,
            buffer,
            allocation,
            size,
            usage,
            pointer,
            is_host_coherent,
        })
    }

    pub(crate) unsafe fn raw(&self) -> vk::Buffer {
        self.buffer
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn usage(&self) -> vk::BufferUsageFlags {
        self.usage
    }

    pub fn map(&self) -> Result<BufferMap<'_>> {
        let Some(pointer) = self.pointer else {
            return Err(anyhow!("buffer not persistently mapped"));
        };
        Ok(BufferMap {
            buffer: self,
            pointer,
        })
    }

    pub fn device_address(&self) -> DeviceAddress<'_> {
        use vulkanalia::prelude::v1_3::*;
        let device = unsafe { self.allocator.device().handle().raw() };
        let info = vk::BufferDeviceAddressInfo::builder().buffer(self.buffer);
        let addr = unsafe { device.get_buffer_device_address(&info) };
        DeviceAddress { buffer: self, addr }
    }

    pub fn flush(&self, offset: u64, size: u64) -> Result<()> {
        if self.is_host_coherent {
            return Ok(());
        }
        #[cfg(debug_assertions)]
        {
            let end = offset
                .checked_add(size)
                .expect("buffer range-check overflow");
            assert!(end <= self.size, "buffer flush range ouf-of-bounds");
        }
        unsafe {
            self.allocator
                .raw()
                .flush_allocation(self.allocation, offset, size)?;
        };
        Ok(())
    }

    pub fn invalidate(&self, offset: u64, size: u64) -> Result<()> {
        if self.is_host_coherent {
            return Ok(());
        }
        #[cfg(debug_assertions)]
        {
            let end = offset
                .checked_add(size)
                .expect("buffer range-check overflow");
            assert!(end <= self.size, "buffer invalidate range ouf-of-bounds");
        }
        unsafe {
            self.allocator
                .raw()
                .invalidate_allocation(self.allocation, offset, size)?;
        };
        Ok(())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.allocator
                .raw()
                .destroy_buffer(self.buffer, self.allocation);
        }
    }
}

pub struct BufferMap<'a> {
    buffer: &'a Buffer,
    pointer: NonNull<u8>,
}

impl<'a> BufferMap<'a> {
    fn pointer_at(&self, offset: u64) -> Result<NonNull<u8>> {
        if offset >= self.buffer.size() {
            return Err(anyhow!("buffer map offset out-of-bounds"));
        }
        Ok(unsafe { self.pointer.add(offset as usize) })
    }

    pub fn copy_from_nonoverlapping(&self, src: &[u8], dst: u64) -> Result<()> {
        let count = src.len();
        let end = dst
            .checked_add(count as u64)
            .context("buffer map bounds overflow")?;
        if end > self.buffer.size() {
            return Err(anyhow!("buffer map size out-of-bounds"));
        }
        let src_ptr = src.as_ptr();
        let dst_ptr = self.pointer_at(dst)?.as_ptr();
        unsafe { std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, count) };
        Ok(())
    }
}

pub struct DeviceAddress<'a> {
    buffer: &'a Buffer,
    addr: u64,
}
