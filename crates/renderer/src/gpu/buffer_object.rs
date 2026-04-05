use std::{cell::RefCell, sync::Arc};

use anyhow::Result;
use bytemuck::Pod;

use crate::gpu::{
    Allocation, AllocatorId, Buffer, BufferSpan, FrameToken, LaneKey, QueueFamilyId, Range,
    RetireToken,
};

pub struct BufferObject {
    layout: slang::LayoutCursor,
    writer: RefCell<BufferWriter>,
}

impl BufferObject {
    pub fn new(layout: &slang::LayoutCursor, writer: BufferWriter) -> Self {
        Self {
            layout: layout.rebase(),
            writer: RefCell::new(writer),
        }
    }

    pub fn cursor(&self) -> BufferCursor<'_> {
        BufferCursor {
            layout: self.layout.clone(),
            writer: &self.writer,
        }
    }

    pub fn finish(self) -> Result<BufferToken> {
        self.writer.into_inner().finish()
    }
}

pub struct BufferWriter {
    span: BufferSpan,
    dirty: Option<Range>,
}

impl BufferWriter {
    pub fn new(span: BufferSpan) -> Self {
        Self { span, dirty: None }
    }

    pub(crate) fn span(&self) -> &BufferSpan {
        &self.span
    }

    fn mark_dirty(&mut self, range: Range) {
        match &mut self.dirty {
            Some(dirty) => {
                let start = dirty.start().min(range.start());
                let end = dirty.end().max(range.end());
                *dirty = Range::new(start, end);
            }
            None => self.dirty = Some(range),
        }
    }

    pub(crate) fn write<T: Pod>(&mut self, layout: &slang::LayoutCursor, value: &T) -> Result<()> {
        let offset = layout.offset().bytes as u64;
        let bytes = bytemuck::bytes_of(value);
        let range = self.span.write_bytes(offset, bytes)?;
        self.mark_dirty(range);
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<BufferToken> {
        if let Some(dirty) = self.dirty {
            self.span.buffer().flush(dirty)?;
        }
        Ok(BufferToken::new(self.span))
    }
}

pub struct BufferCursor<'obj> {
    layout: slang::LayoutCursor,
    writer: &'obj RefCell<BufferWriter>,
}

impl<'obj> BufferCursor<'obj> {
    pub fn field(&self, name: &str) -> Result<Self> {
        Ok(Self {
            layout: self.layout.field(name)?,
            writer: self.writer,
        })
    }

    pub fn index(&self, index: usize) -> Result<Self> {
        Ok(Self {
            layout: self.layout.index(index)?,
            writer: self.writer,
        })
    }

    pub fn set<T: Pod>(&self, value: T) -> Result<()> {
        self.write(&value)
    }

    pub fn write<T: Pod>(&self, value: &T) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        writer.write(&self.layout, value)
    }
}

// TODO: need some generic interface for tokens that are "used" by command
// buffers, to pass-through calls to touch()
pub struct BufferToken {
    owner: Option<QueueFamilyId>,
    retire: RetireToken<Allocation>,
    buffer: Arc<Buffer>,
    allocator: AllocatorId,
    range: Range,
    access: BufferAccess,
}

impl BufferToken {
    pub fn new(span: BufferSpan) -> Self {
        let (buffer, allocator, handle, range) = span.into_parts();
        let allocation = Allocation::new(handle, range);
        let retire = RetireToken::new(allocation);
        let access = BufferAccess::HostWrite;
        Self {
            owner: None,
            retire,
            buffer,
            allocator,
            range,
            access,
        }
    }

    pub fn owner(&self) -> Option<QueueFamilyId> {
        self.owner
    }

    pub fn retire(&self) -> &RetireToken<Allocation> {
        &self.retire
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    pub fn allocator(&self) -> AllocatorId {
        self.allocator
    }

    pub fn range(&self) -> Range {
        self.range
    }

    pub fn access(&self) -> BufferAccess {
        self.access
    }

    // TODO: trait?
    pub fn touch(&mut self, key: LaneKey, frame: &FrameToken) {
        self.retire.touch(key, frame);
    }

    pub fn into_retire(self) -> RetireToken<Allocation> {
        self.retire
    }
}

// XXX
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BufferAccess {
    HostWrite,
    TransferRead,
    TransferWrite,
    UniformRead,
    StorageRead,
    StorageWrite,
    // Vertex
    IndexRead,
    IndirectRead,
}
