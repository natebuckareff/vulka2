use std::cell::RefCell;

use anyhow::{Context, Result, anyhow};
use slang::LayoutCursor;

use crate::gpu::{AllocatorInfo, BufferSpan, ByteWritable, Range, RetireToken, ShaderCursor};

pub struct BufferObject<Handle: Copy> {
    layout: LayoutCursor,
    writer: RefCell<BufferWriter<Handle>>,
}

impl<Handle: Copy> BufferObject<Handle> {
    pub fn new(layout: &LayoutCursor, writer: BufferWriter<Handle>) -> Self {
        Self {
            layout: layout.rebase(),
            writer: RefCell::new(writer),
        }
    }

    pub fn cursor(&self) -> ShaderCursor<'_, BufferWriter<Handle>> {
        ShaderCursor::new(self.layout.clone(), &self.writer)
    }

    pub fn finish(self) -> Result<BufferToken<Handle>> {
        self.writer.into_inner().finish()
    }
}

pub struct BufferWriter<Handle: Copy> {
    span: BufferSpan<Handle>,
    dirty: Option<Range>,
}

impl<Handle: Copy> BufferWriter<Handle> {
    pub(crate) fn new(span: BufferSpan<Handle>) -> Self {
        Self { span, dirty: None }
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

    pub fn finish(self) -> Result<BufferToken<Handle>> {
        if let Some(dirty) = self.dirty {
            self.span.allocation().allocator().buffer().flush(dirty)?;
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

        let range = self.span.range();

        if range.size() == 0 {
            return Err(anyhow!("write to empty buffer span"));
        }

        let write_offset = layout.offset().bytes as u64;
        let write_start = range
            .start()
            .checked_add(write_offset)
            .context("buffer span write overflow")?;

        let write_range = Range::sized(write_start, size as u64)?;

        if !range.fits(write_range) {
            return Err(anyhow!("buffer span write out-of-bounds"));
        }

        self.span
            .allocation()
            .allocator()
            .buffer()
            .map()?
            .copy_from_nonoverlapping(bytes, write_start)?;

        self.mark_dirty(write_range);

        Ok(())
    }
}

// TODO: need some generic interface for tokens that are "used" by command
// buffers, to pass-through calls to touch()
pub struct BufferToken<T: Copy> {
    allocator: AllocatorInfo,
    retire: RetireToken<T>,
    // TODO: will need additional information here to emit correct pipeline
    // barriers and wait on any semaphores for inter-queue use
}

impl<T: Copy> BufferToken<T> {
    pub fn new(span: BufferSpan<T>) -> Self {
        let (allocation, _) = span.into_parts();
        let (allocator, handle) = allocation.into_parts();
        let retire = RetireToken::new(handle);
        Self { allocator, retire }
    }

    pub fn allocator(&self) -> &AllocatorInfo {
        &self.allocator
    }

    pub fn into_parts(self) -> (AllocatorInfo, RetireToken<T>) {
        (self.allocator, self.retire)
    }
}
