use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::gpu::{
    AllocHandle, AllocatorId, Buffer, BufferAllocator, BufferObject, BufferWriter, Range,
};

pub struct BufferSpan {
    buffer: Arc<Buffer>,
    allocator: AllocatorId,
    handle: AllocHandle,
    range: Range,
}

impl BufferSpan {
    pub fn from_buffer(buffer: Buffer) -> Self {
        let size = buffer.size();
        let buffer = Arc::new(buffer);
        Self {
            buffer,
            allocator: AllocatorId::buffer(),
            handle: AllocHandle::dummy(),
            range: Range::new(0, size),
        }
    }

    pub fn from_allocator(
        allocator: &impl BufferAllocator,
        handle: AllocHandle,
        range: Range,
    ) -> Self {
        Self {
            buffer: allocator.backing().buffer().clone(),
            allocator: allocator.id(),
            handle,
            range,
        }
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    pub fn allocator(&self) -> AllocatorId {
        self.allocator
    }

    pub fn handle(&self) -> AllocHandle {
        self.handle
    }

    pub fn range(&self) -> Range {
        self.range
    }

    pub fn write_bytes(&mut self, offset: u64, bytes: &[u8]) -> Result<Range> {
        let size = bytes.len();
        if size == 0 {
            return Ok(Range::new(0, 0));
        }

        if self.range.size() == 0 {
            return Err(anyhow!("write to empty buffer span"));
        }

        let write_start = self.range.start() + offset;
        let write_end = write_start + bytes.len() as u64;
        let write_range = Range::new(write_start, write_end);

        if !self.range.fits(write_range) {
            return Err(anyhow!("buffer span write out-of-bounds"));
        }

        self.buffer
            .map()?
            .copy_from_nonoverlapping(bytes, write_start)?;

        Ok(write_range)
    }

    pub fn writer(self) -> BufferWriter {
        BufferWriter::new(self)
    }

    pub fn object<'reg>(self, layout: &slang::LayoutCursor) -> BufferObject {
        let writer = self.writer();
        BufferObject::new(layout, writer)
    }

    pub fn into_parts(self) -> (Arc<Buffer>, AllocatorId, AllocHandle, Range) {
        let BufferSpan {
            buffer,
            allocator,
            handle,
            range,
        } = self;
        (buffer, allocator, handle, range)
    }
}
