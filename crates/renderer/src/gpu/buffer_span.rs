use anyhow::{Context, Result};

use crate::gpu::{AllocatorInfo, Buffer, BufferAllocator, BufferObject, BufferWriter};

pub struct BufferSpan<Handle: Copy> {
    allocation: Allocation<Handle>,
    range: Range,
}

impl<Handle: Copy> BufferSpan<Handle> {
    pub fn from_buffer(buffer: Buffer, handle: Handle, range: Range) -> Self {
        let allocation = Allocation {
            allocator: AllocatorInfo::from_buffer(buffer),
            handle,
        };
        Self { allocation, range }
    }

    pub fn from_allocator(allocator: &impl BufferAllocator, handle: Handle, range: Range) -> Self {
        let allocation = Allocation {
            allocator: AllocatorInfo::from_allocator(allocator),
            handle,
        };
        Self { allocation, range }
    }

    pub fn allocation(&self) -> &Allocation<Handle> {
        &self.allocation
    }

    pub fn range(&self) -> Range {
        self.range
    }

    pub fn writer(self) -> BufferWriter<Handle> {
        BufferWriter::new(self)
    }

    pub fn object(self, layout: &slang::LayoutCursor) -> BufferObject<Handle> {
        let writer = self.writer();
        BufferObject::new(layout, writer)
    }

    pub fn into_parts(self) -> (Allocation<Handle>, Range) {
        (self.allocation, self.range)
    }
}

pub struct Allocation<Handle: Copy> {
    allocator: AllocatorInfo,
    handle: Handle,
}

impl<Handle: Copy> Allocation<Handle> {
    pub fn allocator(&self) -> &AllocatorInfo {
        &self.allocator
    }

    pub fn handle(&self) -> Handle {
        self.handle
    }

    // TODO: consistent naming everywhere
    pub fn into_parts(self) -> (AllocatorInfo, Handle) {
        (self.allocator, self.handle)
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
