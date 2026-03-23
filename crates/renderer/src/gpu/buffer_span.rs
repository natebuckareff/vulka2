use std::{marker::PhantomData, sync::Arc};

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{Buffer, ByteWritable, RetireToken};

pub struct BufferSpan<Handle: Copy> {
    buffer: Arc<Buffer>,
    handle: Handle,
    offset: u64,
    size: u64,
    marker: PhantomData<Handle>,
}

impl<Handle: Copy> BufferSpan<Handle> {
    pub fn new(buffer: Arc<Buffer>, handle: Handle, offset: u64, size: u64) -> Result<Self> {
        if offset > buffer.size() {
            return Err(anyhow!("buffer span offset is out-of-bounds"));
        }
        let end = offset.checked_add(size).context("buffer span overflow")?;
        if end > buffer.size() {
            return Err(anyhow!("buffer span size is out-of-bounds"));
        }
        Ok(Self {
            buffer,
            handle,
            offset,
            size,
            marker: PhantomData,
        })
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    pub fn handle(&self) -> Handle {
        self.handle
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn usage(&self) -> vk::BufferUsageFlags {
        self.buffer.usage()
    }

    pub fn write(&mut self) -> BufferWriter<'_, Handle> {
        BufferWriter::new(self)
    }
}

// TODO: will implement ByteWritable
struct BufferWriter<'a, Handle: Copy> {
    span: &'a mut BufferSpan<Handle>,
    dirty: Option<(u64, u64)>,
}

impl<'a, Handle: Copy> BufferWriter<'a, Handle> {
    fn new(span: &'a mut BufferSpan<Handle>) -> Self {
        Self { span, dirty: None }
    }

    fn mark_dirty(&mut self, offset: u64, size: u64) -> Result<()> {
        match &mut self.dirty {
            Some((dirty_start, dirty_end)) => {
                let end = offset.checked_add(size).context("mark dirty overflow")?;
                *dirty_start = (*dirty_start).min(offset);
                *dirty_end = (*dirty_end).max(end);
            }
            None => {
                let end = offset.checked_add(size).context("mark dirty overflow")?;
                self.dirty = Some((offset, end))
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<BufferToken<Handle>> {
        if let Some((start, end)) = self.dirty {
            let offset = self
                .span
                .offset
                .checked_add(start)
                .context("bump allocator finish overflow")?;
            let size = end.checked_sub(start).context("bump allocator undeflow")?;
            self.span.buffer.flush(offset, size)?;
        }
        Ok(BufferToken::new(self.span.handle))
    }
}

impl<'a, Handle: Copy> ByteWritable for BufferWriter<'a, Handle> {
    fn write_pod<P: bytemuck::Pod>(&mut self, layout: &slang::LayoutCursor, pod: &P) -> Result<()> {
        self.write_bytes(layout, bytemuck::bytes_of(pod))
    }

    fn write_bytes(&mut self, layout: &slang::LayoutCursor, bytes: &[u8]) -> Result<()> {
        let count = bytes.len();

        if count == 0 {
            return Ok(());
        }

        if self.span.size() == 0 {
            return Err(anyhow!("write to empty buffer span"));
        }

        let size = count as u64;
        let write_offset = layout.offset().bytes as u64;
        let write_end = write_offset
            .checked_add(size)
            .context("buffer write bounds overflow")?;

        if write_end > self.span.size() {
            return Err(anyhow!("buffer span write out-of-bounds"));
        }

        let dst_offset = self
            .span
            .offset
            .checked_add(write_offset)
            .context("buffer span offset overflow")?;

        self.span
            .buffer
            .map()?
            .copy_from_nonoverlapping(bytes, dst_offset)?;

        self.mark_dirty(write_offset, size)?;

        Ok(())
    }
}

struct BufferToken<T: Copy> {
    retire: RetireToken<T>,
    // TODO: will need additional information here to emit correct pipeline
    // barriers and wait on any semaphores for iter-queue use
}

impl<T: Copy> BufferToken<T> {
    fn new(handle: T) -> Self {
        let retire = RetireToken::new(handle);
        Self { retire }
    }
}
