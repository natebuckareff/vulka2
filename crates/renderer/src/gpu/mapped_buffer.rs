use std::sync::Arc;
use std::ptr::NonNull;

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk;

use super::{GpuBuffer, GpuBufferView};

#[derive(Clone)]
pub struct MappedBuffer {
    inner: Arc<MappedBufferInner>,
}

struct MappedBufferInner {
    buffer: GpuBuffer,
    mapped: NonNull<u8>,
}

impl MappedBuffer {
    pub fn new(buffer: GpuBuffer) -> Result<Self> {
        let raw_ptr = unsafe { buffer.allocator().map_memory(buffer.allocation()) }
            .map_err(|err| anyhow!(err))
            .context("failed to map buffer allocation")?;
        let mapped = NonNull::new(raw_ptr as *mut u8)
            .ok_or_else(|| anyhow!("mapped buffer pointer is null"))?;

        Ok(Self {
            inner: Arc::new(MappedBufferInner { buffer, mapped }),
        })
    }

    pub fn buffer(&self) -> &GpuBuffer {
        &self.inner.buffer
    }

    pub fn view(&self) -> GpuBufferView {
        self.inner.buffer.whole_view()
    }

    pub(crate) fn write_and_flush(
        &self,
        dst: &GpuBufferView,
        dst_local_offset: vk::DeviceSize,
        bytes: &[u8],
    ) -> Result<()> {
        if self.inner.buffer.handle() != dst.handle() {
            return Err(anyhow!("mapped buffer write target does not match destination view"));
        }

        let size = bytes.len() as vk::DeviceSize;
        let target = dst.subview(dst_local_offset, size)?;
        let absolute_offset = target.offset();
        let absolute_offset_usize = usize::try_from(absolute_offset)
            .map_err(|_| anyhow!("mapped write offset does not fit in usize"))?;

        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.inner.mapped.as_ptr().add(absolute_offset_usize),
                bytes.len(),
            );
        }

        unsafe {
            self.inner
                .buffer
                .allocator()
                .flush_allocation(self.inner.buffer.allocation(), absolute_offset, size)
        }
        .map_err(|err| anyhow!(err))
        .context("failed to flush mapped buffer allocation")
    }
}

impl Drop for MappedBufferInner {
    fn drop(&mut self) {
        unsafe {
            self.buffer.allocator().unmap_memory(self.buffer.allocation());
        }
    }
}
