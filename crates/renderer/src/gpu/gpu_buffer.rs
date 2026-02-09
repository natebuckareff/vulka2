use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use bytemuck::Pod;
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;
use vulkanalia_vma::{self as vma, Alloc, AllocationCreateFlags};

use super::GpuDevice;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuBufferWriteMode {
    HostVisibleTransient,
    HostVisiblePersistent,
    DeviceLocalOnly,
}

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
    allocator: Arc<vma::Allocator>,
    buffer: vk::Buffer,
    allocation: vma::Allocation,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    write_mode: GpuBufferWriteMode,
    // This pointer is populated only for persistently mapped allocations.
    // Mapping ownership stays with VMA allocation flags, not this wrapper.
    mapped_ptr: Option<NonNull<u8>>,
    write_lock: Mutex<()>,
}

impl GpuBuffer {
    // Project rule: memory allocated by VMA must only be mapped/unmapped via VMA APIs.
    pub fn create(
        allocator: Arc<vma::Allocator>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        allocation_options: &vma::AllocationOptions,
        write_mode: GpuBufferWriteMode,
    ) -> Result<Self> {
        let allocation_options =
            Self::normalize_allocation_options(allocation_options, write_mode)?;
        let buffer_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (buffer, allocation) = unsafe { allocator.create_buffer(buffer_info, &allocation_options) }
            .map_err(|err| anyhow!(err))
            .context("failed to create VMA buffer")?;

        let mapped_ptr = match write_mode {
            GpuBufferWriteMode::HostVisiblePersistent => {
                let info = allocator.get_allocation_info(allocation);
                let Some(mapped_ptr) = NonNull::new(info.pMappedData as *mut u8) else {
                    unsafe {
                        allocator.destroy_buffer(buffer, allocation);
                    }
                    return Err(anyhow!(
                        "persistent buffer allocation was not mapped as expected"
                    ));
                };
                Some(mapped_ptr)
            }
            _ => None,
        };

        Ok(Self {
            inner: Arc::new(GpuBufferInner {
                allocator,
                buffer,
                allocation,
                size,
                usage,
                write_mode,
                mapped_ptr,
                write_lock: Mutex::new(()),
            }),
        })
    }

    fn normalize_allocation_options(
        requested: &vma::AllocationOptions,
        write_mode: GpuBufferWriteMode,
    ) -> Result<vma::AllocationOptions> {
        let mut options = *requested;

        let host_access_flags =
            AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE | AllocationCreateFlags::HOST_ACCESS_RANDOM;
        let has_host_access = options.flags.intersects(host_access_flags);

        match write_mode {
            GpuBufferWriteMode::HostVisibleTransient => {
                if !has_host_access {
                    options.flags |= AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE;
                }
                if options.flags.contains(AllocationCreateFlags::MAPPED) {
                    return Err(anyhow!(
                        "HostVisibleTransient buffers must not set MAPPED allocation flag"
                    ));
                }
            }
            GpuBufferWriteMode::HostVisiblePersistent => {
                if !has_host_access {
                    options.flags |= AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE;
                }
                options.flags |= AllocationCreateFlags::MAPPED;
            }
            GpuBufferWriteMode::DeviceLocalOnly => {
                let forbidden =
                    host_access_flags | AllocationCreateFlags::MAPPED | AllocationCreateFlags::HOST_ACCESS_ALLOW_TRANSFER_INSTEAD;
                if options.flags.intersects(forbidden) {
                    return Err(anyhow!(
                        "DeviceLocalOnly buffers must not include host-access or mapped allocation flags"
                    ));
                }
            }
        }

        Ok(options)
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

    pub fn write_mode(&self) -> GpuBufferWriteMode {
        self.inner.write_mode
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

    pub fn write_pod<T: Pod>(&self, offset: usize, value: &T) -> Result<()> {
        self.write_bytes(offset, bytemuck::bytes_of(value))
    }

    pub fn write_slice<T: Pod>(&self, offset: usize, values: &[T]) -> Result<()> {
        self.write_bytes(offset, bytemuck::cast_slice(values))
    }

    /// Writes CPU data into this buffer allocation and flushes the touched range.
    ///
    /// This function only handles host-memory visibility. Callers remain
    /// responsible for GPU-side ordering/synchronization (fences/barriers) before
    /// the written range is consumed by GPU work.
    pub fn write_bytes(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        let (start, size) = self.checked_write_range(offset, bytes.len())?;
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| anyhow!("buffer write lock poisoned"))?;

        match self.inner.write_mode {
            GpuBufferWriteMode::HostVisibleTransient => {
                self.write_mapped_once(offset, bytes, start, size)
            }
            GpuBufferWriteMode::HostVisiblePersistent => {
                self.write_persistent(offset, bytes, start, size)
            }
            GpuBufferWriteMode::DeviceLocalOnly => Err(anyhow!(
                "buffer is device-local only and cannot be written by CPU"
            )),
        }
    }

    fn checked_write_range(
        &self,
        offset: usize,
        byte_len: usize,
    ) -> Result<(vk::DeviceSize, vk::DeviceSize)> {
        let start = vk::DeviceSize::try_from(offset)
            .map_err(|_| anyhow!("buffer write offset does not fit in device size"))?;
        let size = vk::DeviceSize::try_from(byte_len)
            .map_err(|_| anyhow!("buffer write size does not fit in device size"))?;
        Self::checked_subrange(self.inner.size, start, size, "buffer write")?;

        Ok((start, size))
    }

    fn checked_subrange(
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

    fn write_mapped_once(
        &self,
        offset: usize,
        bytes: &[u8],
        start: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Result<()> {
        let ptr = unsafe { self.inner.allocator.map_memory(self.inner.allocation) }
            .map_err(|err| anyhow!(err))
            .context("failed to map buffer allocation")?;
        let Some(mapped_ptr) = NonNull::new(ptr as *mut u8) else {
            unsafe {
                self.inner.allocator.unmap_memory(self.inner.allocation);
            }
            return Err(anyhow!("transient buffer mapping returned a null pointer"));
        };

        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped_ptr.as_ptr().add(offset), bytes.len());
        }

        let flush_result = unsafe { self.inner.allocator.flush_allocation(self.inner.allocation, start, size) };
        unsafe {
            self.inner.allocator.unmap_memory(self.inner.allocation);
        }

        flush_result
            .map_err(|err| anyhow!(err))
            .context("failed to flush buffer allocation")
    }

    fn write_persistent(
        &self,
        offset: usize,
        bytes: &[u8],
        start: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Result<()> {
        let mapped_ptr = self
            .inner
            .mapped_ptr
            .ok_or_else(|| anyhow!("persistent buffer has no mapped pointer"))?;

        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped_ptr.as_ptr().add(offset), bytes.len());
        }

        unsafe { self.inner.allocator.flush_allocation(self.inner.allocation, start, size) }
            .map_err(|err| anyhow!(err))
            .context("failed to flush persistent buffer allocation")
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

    pub fn write_pod<T: Pod>(&self, offset: usize, value: &T) -> Result<()> {
        self.write_bytes(offset, bytemuck::bytes_of(value))
    }

    pub fn write_slice<T: Pod>(&self, offset: usize, values: &[T]) -> Result<()> {
        self.write_bytes(offset, bytemuck::cast_slice(values))
    }

    pub fn write_bytes(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        let local_offset = vk::DeviceSize::try_from(offset)
            .map_err(|_| anyhow!("buffer view write offset does not fit in device size"))?;
        let local_size = vk::DeviceSize::try_from(bytes.len())
            .map_err(|_| anyhow!("buffer view write size does not fit in device size"))?;

        GpuBuffer::checked_subrange(self.size, local_offset, local_size, "buffer view write")?;

        let absolute_offset = self
            .offset
            .checked_add(local_offset)
            .ok_or_else(|| anyhow!("buffer view write offset overflow"))?;
        let absolute_offset = usize::try_from(absolute_offset)
            .map_err(|_| anyhow!("buffer view write offset does not fit in usize"))?;

        self.buffer.write_bytes(absolute_offset, bytes)
    }
}

impl Drop for GpuBufferInner {
    fn drop(&mut self) {
        unsafe {
            self.allocator.destroy_buffer(self.buffer, self.allocation);
        }
    }
}
