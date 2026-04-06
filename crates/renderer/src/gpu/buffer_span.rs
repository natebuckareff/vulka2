use std::sync::Arc;

use anyhow::Result;

use crate::gpu::{
    AllocHandle, AllocatorId, Buffer, BufferAllocator, BufferObject, BufferWriter, Range,
    StorageSpan,
};

pub struct BufferSpan {
    buffer: Arc<Buffer>,
    allocator: AllocatorId,
    handle: AllocHandle,
    range: Range,
}

impl BufferSpan {
    pub(crate) fn from_buffer(buffer: Buffer) -> Self {
        let size = buffer.size();
        let buffer = Arc::new(buffer);
        Self {
            buffer,
            allocator: AllocatorId::buffer(),
            handle: AllocHandle::dummy(),
            range: Range::new(0, size),
        }
    }

    pub(crate) fn from_allocator(
        allocator: &impl BufferAllocator,
        handle: AllocHandle,
        range: Range,
    ) -> Self {
        Self {
            buffer: allocator.storage().span().buffer().clone(),
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

    pub fn writer(self) -> Result<BufferWriter> {
        BufferWriter::new(self)
    }

    pub fn object<'reg>(self, layout: &slang::LayoutCursor) -> Result<BufferObject> {
        let writer = self.writer()?;
        Ok(BufferObject::new(layout, writer))
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
