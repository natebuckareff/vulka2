use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{Buffer, BufferObject, ByteWritable, RetireToken};

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

    pub fn range(&self) -> &Range {
        &self.range
    }

    pub fn usage(&self) -> vk::BufferUsageFlags {
        self.buffer.usage()
    }

    pub fn object(self, layout: &slang::LayoutCursor) -> BufferObject<Handle> {
        let writer = BufferWriter::new(self);
        BufferObject::new(layout, writer)
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

// TODO: will implement ByteWritable
pub struct BufferWriter<Handle: Copy> {
    span: BufferSpan<Handle>,
    dirty: Option<Range>,
}

impl<Handle: Copy> BufferWriter<Handle> {
    fn new(span: BufferSpan<Handle>) -> Self {
        Self { span, dirty: None }
    }

    fn mark_dirty(&mut self, range: Range) {
        match &mut self.dirty {
            Some(dirty) => {
                dirty.start = dirty.start.min(range.start);
                dirty.end = dirty.end.max(range.end);
            }
            None => self.dirty = Some(range),
        }
    }

    pub fn finish(self) -> Result<BufferToken<Handle>> {
        if let Some(dirty) = self.dirty {
            self.span.buffer.flush(dirty)?;
        }
        Ok(BufferToken::new(self.span))
    }
}

impl<Handle: Copy> ByteWritable for BufferWriter<Handle> {
    fn write_bytes(&mut self, layout: &slang::LayoutCursor, bytes: &[u8]) -> Result<()> {
        let size = bytes.len();

        if size == 0 {
            return Ok(());
        }

        if self.span.range.size() == 0 {
            return Err(anyhow!("write to empty buffer span"));
        }

        let write_offset = layout.offset().bytes as u64;
        let write_start = self
            .span
            .range()
            .start()
            .checked_add(write_offset)
            .context("buffer span write overflow")?;

        let write_range = Range::sized(write_start, size as u64)?;

        if !self.span.range.fits(write_range) {
            return Err(anyhow!("buffer span write out-of-bounds"));
        }

        self.span
            .buffer
            .map()?
            .copy_from_nonoverlapping(bytes, write_start)?;

        self.mark_dirty(write_range);

        Ok(())
    }
}

pub struct BufferToken<T: Copy> {
    id: Option<AllocId>,
    buffer: Arc<Buffer>,
    retire: RetireToken<T>,
    // TODO: will need additional information here to emit correct pipeline
    // barriers and wait on any semaphores for inter-queue use
}

impl<T: Copy> BufferToken<T> {
    pub fn new(span: BufferSpan<T>) -> Self {
        let retire = RetireToken::new(span.handle);
        Self {
            id: span.id,
            buffer: span.buffer,
            retire,
        }
    }

    pub fn id(&self) -> Option<AllocId> {
        self.id
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    pub fn parts(self) -> (Arc<Buffer>, RetireToken<T>) {
        (self.buffer, self.retire)
    }
}
