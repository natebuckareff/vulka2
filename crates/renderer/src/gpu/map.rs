use std::ptr::NonNull;

use anyhow::{Context, Result, anyhow};

use crate::gpu::{Buffer, BufferSpan, Range};

pub struct BufferMap<'a> {
    buffer: &'a Buffer,
    pointer: NonNull<u8>,
}

impl<'a> BufferMap<'a> {
    pub(crate) unsafe fn new(buffer: &'a Buffer) -> Result<Self> {
        let pointer = unsafe { buffer.pointer()? };
        Ok(Self { buffer, pointer })
    }

    fn pointer_at(&self, offset: u64) -> Result<NonNull<u8>> {
        if offset > self.buffer.size() {
            return Err(anyhow!("buffer map offset out-of-bounds"));
        }
        Ok(unsafe { self.pointer.add(offset as usize) })
    }

    pub fn copy_from_nonoverlapping(&self, src: &[u8], dst: u64) -> Result<()> {
        let count = src.len();
        let end = dst
            .checked_add(count as u64)
            .context("buffer map bounds overflow")?;
        if end > self.buffer.size() {
            return Err(anyhow!("buffer map copy out-of-bounds"));
        }
        let src_ptr = src.as_ptr();
        let dst_ptr = self.pointer_at(dst)?.as_ptr();
        unsafe { std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, count) };
        Ok(())
    }

    pub fn copy_into_nonoverlapping(&self, src: u64, dst: &mut [u8]) -> Result<()> {
        let count = dst.len();
        let end = src
            .checked_add(count as u64)
            .context("buffer map bounds overflow")?;
        if end > self.buffer.size() {
            return Err(anyhow!("buffer map copy out-of-bounds"));
        }
        let src_ptr = self.pointer_at(src)?.as_ptr();
        let dst_ptr = dst.as_mut_ptr();
        unsafe { std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, count) };
        Ok(())
    }
}

pub struct MapSpan {
    // SAFETY: map first so it is destructed before the Arc<Buffer> in span is
    // dropped
    map: BufferMap<'static>,
    span: BufferSpan,
}

impl MapSpan {
    pub fn new(span: BufferSpan) -> Result<Self> {
        // SAFETY: span holds an Arc<Buffer> so span and map always live as long
        // as each other
        let map = unsafe { std::mem::transmute(span.buffer().map()?) };
        Ok(Self { map, span })
    }

    pub fn span(&self) -> &BufferSpan {
        &self.span
    }

    pub fn read_bytes(&mut self, offset: u64, bytes: &mut [u8]) -> Result<Range> {
        let span = &self.span;
        let size = bytes.len();
        if size == 0 {
            return Ok(Range::new(0, 0));
        }

        let range = span.range();
        if range.size() == 0 {
            return Err(anyhow!("read empty buffer span"));
        }

        let read_start = range.start() + offset;
        let read_end = read_start + bytes.len() as u64;
        let read_range = Range::new(read_start, read_end);

        if !range.fits(read_range) {
            return Err(anyhow!("buffer span read out-of-bounds"));
        }

        self.map.copy_into_nonoverlapping(read_start, bytes)?;

        Ok(read_range)
    }

    pub fn write_bytes(&mut self, offset: u64, bytes: &[u8]) -> Result<Range> {
        let span = &self.span;
        let size = bytes.len();
        if size == 0 {
            return Ok(Range::new(0, 0));
        }

        let range = span.range();
        if range.size() == 0 {
            return Err(anyhow!("write to empty buffer span"));
        }

        let write_start = range.start() + offset;
        let write_end = write_start + bytes.len() as u64;
        let write_range = Range::new(write_start, write_end);

        if !range.fits(write_range) {
            return Err(anyhow!("buffer span write out-of-bounds"));
        }

        self.map.copy_from_nonoverlapping(bytes, write_start)?;

        Ok(write_range)
    }

    pub fn into_span(self) -> BufferSpan {
        self.span
    }
}
