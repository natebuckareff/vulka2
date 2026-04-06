use std::{cell::RefCell, sync::Arc};

use anyhow::{Context, Result};
use bytemuck::Pod;

use crate::gpu::{
    Allocation, AllocatorId, Buffer, BufferSpan, FrameToken, LaneKey, Map, QueueFamilyId, Range,
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
    map: Map,
}

impl BufferWriter {
    pub fn new(map: Map) -> Result<Self> {
        Ok(Self { map })
    }

    pub(crate) fn map(&self) -> &Map {
        &self.map
    }

    pub(crate) fn write<T: Pod>(&mut self, layout: &slang::LayoutCursor, value: &T) -> Result<()> {
        let offset = layout.offset().bytes as u64;
        let bytes = bytemuck::bytes_of(value);
        let end = offset
            .checked_add(bytes.len() as u64)
            .context("buffer writer overflow")? as u64;
        self.map[offset..end].copy_from_slice(bytes);
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<BufferToken> {
        Ok(BufferToken::new(self.map.into_span()?))
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
