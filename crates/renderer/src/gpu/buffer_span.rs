use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use vulkanalia::vk;

use crate::gpu::{Buffer, BufferObject, BufferWriter};

pub struct BufferSpan<Handle: Copy> {
    id: Option<AllocId>,
    buffer: Arc<Buffer>,
    handle: Handle,
    range: Range,
}

impl<Handle: Copy> BufferSpan<Handle> {
    pub fn new(id: Option<AllocId>, buffer: Arc<Buffer>, handle: Handle, range: Range) -> Self {
        debug_assert!(buffer.fits(range));
        Self {
            id,
            buffer,
            handle,
            range,
        }
    }

    pub fn id(&self) -> Option<AllocId> {
        self.id
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    pub fn handle(&self) -> Handle {
        self.handle
    }

    pub fn range(&self) -> Range {
        self.range
    }

    pub fn usage(&self) -> vk::BufferUsageFlags {
        self.buffer.usage()
    }

    pub fn writer(self) -> BufferWriter<Handle> {
        BufferWriter::new(self)
    }

    pub fn object(self, layout: &slang::LayoutCursor) -> BufferObject<Handle> {
        let writer = self.writer();
        BufferObject::new(layout, writer)
    }

    pub fn parts(self) -> (Option<AllocId>, Arc<Buffer>, Handle, Range) {
        (self.id, self.buffer, self.handle, self.range)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AllocId(u64);

impl AllocId {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range {
    start: u64,
    end: u64,
}

impl Range {
    pub fn new(start: u64, end: u64) -> Self {
        debug_assert!(start <= end, "invalid range");
        Self { start, end }
    }

    pub fn sized(start: u64, size: u64) -> Result<Self> {
        let end = start.checked_add(size).context("range overflow")?;
        Ok(Self { start, end })
    }

    pub fn empty() -> Self {
        Self { start: 0, end: 0 }
    }

    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn end(&self) -> u64 {
        self.end
    }

    pub fn size(&self) -> u64 {
        // OVERFLOW: since end is always > start, this will never overflow
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end == self.start
    }

    pub fn fits(&self, other: Range) -> bool {
        other.start >= self.start && other.end <= self.end
    }

    pub fn clamp(&self, other: Range) -> Range {
        let start = self.start.clamp(other.start, other.end);
        let end = self.end.clamp(other.start, other.end);
        Range { start, end }
    }

    pub fn add(&self, offset: u64) -> Result<Range> {
        Ok(Self {
            start: self
                .start
                .checked_add(offset)
                .context("range add overflow")?,
            end: self.end.checked_add(offset).context("range add overflow")?,
        })
    }

    pub fn sub(&self, offset: u64) -> Result<Range> {
        Ok(Self {
            start: self
                .start
                .checked_sub(offset)
                .context("range sub overflow")?,
            end: self.end.checked_sub(offset).context("range sub overflow")?,
        })
    }
}

pub struct AlignedRange {
    start: u64,
    aligned: Range,
}

impl AlignedRange {
    pub fn new(start: u64, aligned: Range) -> Self {
        debug_assert!(start <= aligned.start);
        Self { start, aligned }
    }

    pub fn full(&self) -> Range {
        Range::new(self.start, self.aligned.end)
    }

    pub fn aligned(&self) -> Range {
        self.aligned
    }
}
