use std::sync::Arc;

use anyhow::{Context, Result};
use vulkanalia::vk;
use vulkanalia_vma as vma;

use crate::gpu::{Buffer, BufferSpan, GpuAllocator};

struct BumpAllocator {
    buffer: Arc<Buffer>,
    offset: u64,
}

impl BumpAllocator {
    pub fn new(
        allocator: Arc<GpuAllocator>,
        capacity: u64,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let flags = vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
            | vma::AllocationCreateFlags::MAPPED;
        let buffer = Arc::new(Buffer::new(allocator, capacity, usage, flags)?);
        Ok(Self { buffer, offset: 0 })
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn capacity(&self) -> u64 {
        self.buffer.size()
    }

    pub fn allocate(&mut self, size: u64, align: Option<u64>) -> Result<BufferSpan<()>> {
        let align = align.unwrap_or(1);
        let offset = align_up(self.offset, align);
        let buffer = self.buffer.clone();
        let span = BufferSpan::new(buffer, (), offset, size)?;
        self.offset = offset
            .checked_add(size)
            .context("bump allocator overflow")?;
        Ok(span)
    }
}

fn align_up(value: u64, align: u64) -> u64 {
    debug_assert!(align.is_power_of_two());
    (value + (align - 1)) & !(align - 1)
}
