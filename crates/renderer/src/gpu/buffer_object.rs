use std::{cell::RefCell, sync::Arc};

use anyhow::{Context, Result, anyhow};
use slang::LayoutCursor;

use crate::gpu::{AllocId, Buffer, BufferSpan, ByteWritable, Range, RetireToken, ShaderCursor};

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
            self.span.buffer().flush(dirty)?;
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

        if self.span.range().size() == 0 {
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

        if !self.span.range().fits(write_range) {
            return Err(anyhow!("buffer span write out-of-bounds"));
        }

        self.span
            .buffer()
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
        let (id, buffer, handle, _) = span.parts();
        let retire = RetireToken::new(handle);
        Self { id, buffer, retire }
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
