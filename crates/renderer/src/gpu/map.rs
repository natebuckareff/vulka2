use std::ptr::NonNull;

use anyhow::{Context, Result, anyhow};

use crate::gpu::{Buffer, BufferSpan, Range};

pub struct MapSpan {
    // SAFETY: buffer first so it is destructed before the Arc<Buffer> in span
    // is dropped
    buffer: &'static Buffer,
    pointer: NonNull<u8>,
    span: BufferSpan,
}

impl MapSpan {
    pub fn new(span: BufferSpan) -> Result<Self> {
        let buf = span.buffer();
        // SAFETY: span holds an Arc<Buffer> so span and buffer always live as
        // long as each other
        let buffer = unsafe { std::mem::transmute(buf.as_ref()) };
        let pointer = unsafe { buf.pointer()? };
        Ok(Self {
            buffer,
            pointer,
            span,
        })
    }

    unsafe fn pointer_at(&self, offset: u64) -> NonNull<u8> {
        assert!(
            offset <= self.buffer.size(),
            "map span offset out-of-bounds"
        );
        unsafe { self.pointer.add(offset as usize) }
    }

    pub fn span(&self) -> &BufferSpan {
        &self.span
    }

    // TODO: confusing naming mixing offsets/pointers and relative/absolute
    pub fn base_pointer(&self) -> u64 {
        self.pointer.as_ptr() as u64
    }

    pub fn effective_range(&self) -> Result<Range> {
        self.span.range().add(self.base_pointer())
    }

    pub fn alignment(&self) -> u64 {
        // OVERFLOW: already checked base and start when this span was allocated
        let addr = self.base_pointer() + self.span.range().start();
        1u64 << addr.trailing_zeros()
    }

    pub fn is_aligned(&self, addr: u64) -> bool {
        addr % self.alignment() == 0
    }

    pub fn read_bytes(&mut self, offset: u64, bytes: &mut [u8]) -> Result<Range> {
        let span = &self.span;
        let size = bytes.len();
        if size == 0 {
            return Ok(Range::new(0, 0));
        }

        let range = span.range();
        if range.size() == 0 {
            return Err(anyhow!("read empty map span"));
        }

        let read_start = range.start() + offset;
        let read_end = read_start + bytes.len() as u64;
        let read_range = Range::new(read_start, read_end);

        if !range.fits(read_range) {
            return Err(anyhow!("map span read out-of-bounds"));
        }

        self.copy_into_nonoverlapping(read_start, bytes)?;

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
            return Err(anyhow!("map span write out-of-bounds"));
        }

        self.copy_from_nonoverlapping(bytes, write_start)?;

        Ok(write_range)
    }

    pub fn copy_into_nonoverlapping(&self, src: u64, dst: &mut [u8]) -> Result<()> {
        let count = dst.len();
        let end = src
            .checked_add(count as u64)
            .context("buffer map bounds overflow")?;
        if end > self.buffer.size() {
            return Err(anyhow!("buffer map copy out-of-bounds"));
        }
        let src_ptr = unsafe { self.pointer_at(src).as_ptr() };
        let dst_ptr = dst.as_mut_ptr();
        unsafe { std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, count) };
        Ok(())
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
        let dst_ptr = unsafe { self.pointer_at(dst).as_ptr() };
        unsafe { std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, count) };
        Ok(())
    }

    pub fn into_span(self) -> BufferSpan {
        self.span
    }
}
