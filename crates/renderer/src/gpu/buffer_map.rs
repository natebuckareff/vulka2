use std::{cell::RefCell, ptr::NonNull};

use anyhow::Result;

use crate::gpu::{Buffer, BufferObject, BufferSpan, BufferWriter, Range};

pub struct BufferMap {
    // SAFETY: buffer first so it is destructed before the Arc<Buffer> in span
    // is dropped
    buffer: &'static Buffer,
    mapping: Mapping,
    base: u64,
    span: BufferSpan,
    dirty: RefCell<Dirty>,
}

impl BufferMap {
    pub fn new(span: BufferSpan) -> Result<Self> {
        let buf = span.buffer();
        // SAFETY: span holds an Arc<Buffer> so span and buffer always live as
        // long as each other
        let buffer = unsafe { std::mem::transmute(buf.as_ref()) };
        let mapping = Mapping::new(buffer)?;
        let base = mapping.translate_offset(span.range().start());
        Ok(Self {
            buffer,
            mapping,
            base,
            span,
            dirty: RefCell::new(Dirty::new()),
        })
    }

    pub fn len(&self) -> u64 {
        self.span.range().size()
    }

    pub fn span(&self) -> &BufferSpan {
        &self.span
    }

    // pre-translated base offset
    pub fn base(&self) -> u64 {
        self.base
    }

    pub fn mapping(&self) -> &Mapping {
        &self.mapping
    }

    pub fn alignment(&self) -> u64 {
        // OVERFLOW: already checked base and start when this span was allocated
        1u64 << self.base.trailing_zeros()
    }

    pub fn is_aligned(&self, addr: u64) -> bool {
        addr % self.alignment() == 0
    }

    pub fn read_bytes(&self, offset: u64, dst: &mut [u8]) -> Result<Range> {
        let range = Range::sized(offset, dst.len() as u64)?;
        let end = range.end();
        dst.copy_from_slice(&self[offset..end]);
        Ok(range)
    }

    pub fn write_bytes(&mut self, offset: u64, bytes: &[u8]) -> Result<Range> {
        let range = Range::sized(offset, bytes.len() as u64)?;
        let end = range.end();
        self[offset..end].copy_from_slice(bytes);
        Ok(range)
    }

    // this and get_range_unchecked_mut are factored out so BufferView can does
    // not double-assert
    pub(crate) unsafe fn get_range_unchecked(&self, range: std::ops::Range<u64>) -> &[u8] {
        // OVERFLOW: will not overflow when `range.start <= range.end`
        let size: u64 = range.end - range.start;

        // OVERFLOW: if `range.start` is in range, then this will not overflow,
        // as the span was already constructed relative to the base offset
        let ptr = (self.base + range.start) as *const u8;
        unsafe { std::slice::from_raw_parts(ptr, size as usize) }
    }

    pub(crate) unsafe fn get_range_unchecked_mut(
        &mut self,
        range: std::ops::Range<u64>,
    ) -> &mut [u8] {
        let span_start = self.span.range().start();
        // OVERFLOW: will not overflow for the same reasoning as below
        let start = span_start + range.start;
        let end = span_start + range.end;
        self.dirty.borrow_mut().mark(Range::new(start, end));

        // OVERFLOW: will not overflow when `range.start <= range.end`
        let size = range.end - range.start;

        // OVERFLOW: if `range.start` is in range, then this will not overflow,
        // as the span was already constructed relative to the base offset
        let ptr = (self.base + range.start) as *mut u8;
        unsafe { std::slice::from_raw_parts_mut(ptr, size as usize) }
    }

    pub fn writer(self) -> Result<BufferWriter> {
        BufferWriter::new(self)
    }

    pub fn object<'reg>(self, layout: &slang::LayoutCursor) -> Result<BufferObject> {
        let writer = self.writer()?;
        Ok(BufferObject::new(layout, writer))
    }

    pub fn into_span(self) -> Result<BufferSpan> {
        if let Some(dirty) = self.dirty.into_inner().range {
            self.span.buffer().flush(dirty)?;
        }
        Ok(self.span)
    }
}

pub struct Mapping {
    pointer: NonNull<u8>,
    size: u64,
}

impl Mapping {
    pub fn new(buffer: &Buffer) -> Result<Self> {
        let pointer = unsafe { buffer.pointer()? };
        let size = buffer.size();
        Ok(Self { pointer, size })
    }

    pub fn base(&self) -> u64 {
        self.pointer.as_ptr() as u64
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn translate_offset(&self, offset: u64) -> u64 {
        assert!(offset <= self.size);
        self.pointer.as_ptr() as u64 + offset
    }

    pub fn translate_range(&self, range: Range) -> Range {
        assert!(range.end() <= self.size);
        let start = self.translate_offset(range.start());
        let end = self.translate_offset(range.end());
        Range::new(start, end)
    }
}

struct Dirty {
    range: Option<Range>,
}

impl Dirty {
    fn new() -> Self {
        Self { range: None }
    }

    fn mark(&mut self, dirty: Range) {
        match &mut self.range {
            Some(range) => {
                let min_start = range.start().min(dirty.start());
                let max_end = range.end().max(dirty.end());
                *range = Range::new(min_start, max_end)
            }
            None => {
                self.range = Some(dirty);
            }
        }
    }

    fn into_range(self) -> Option<Range> {
        self.range
    }
}

impl Drop for Dirty {
    fn drop(&mut self) {
        if let Some(range) = self.range {
            debug_assert!(range.size() == 0, "buffer map did not flush ranges");
        }
    }
}

impl std::ops::Index<u64> for BufferMap {
    type Output = u8;

    fn index(&self, index: u64) -> &Self::Output {
        assert!(index < self.len(), "buffer map index out-of-bounds");
        unsafe {
            // OVERFLOW: if `index` is in range, then this will not overflow, as
            // the span was already constructed relative to the base offset
            let ptr = (self.base + index) as *const u8;
            &*ptr
        }
    }
}

impl std::ops::IndexMut<u64> for BufferMap {
    fn index_mut(&mut self, index: u64) -> &mut Self::Output {
        assert!(index < self.len(), "buffer map index out-of-bounds");
        unsafe {
            let start = self.span.range().start() + index;
            self.dirty.borrow_mut().mark(Range::new(start, start + 1));

            // OVERFLOW: if `index` is in range, then this will not overflow, as
            // the span was already constructed relative to the base offset
            let ptr = (self.base + index) as *mut u8;
            &mut *ptr
        }
    }
}

impl std::ops::Index<std::ops::Range<u64>> for BufferMap {
    type Output = [u8];

    fn index(&self, range: std::ops::Range<u64>) -> &Self::Output {
        assert!(range.start <= self.len(), "buffer map index out-of-bounds");
        assert!(range.end <= self.len(), "buffer map index out-of-bounds");
        assert!(range.start <= range.end, "invalid buffer map range");

        // SAFETY: asserts guard this
        unsafe { self.get_range_unchecked(range) }
    }
}

impl std::ops::IndexMut<std::ops::Range<u64>> for BufferMap {
    fn index_mut(&mut self, range: std::ops::Range<u64>) -> &mut Self::Output {
        assert!(range.start <= self.len(), "buffer map index out-of-bounds");
        assert!(range.end <= self.len(), "buffer map index out-of-bounds");
        assert!(range.start <= range.end, "invalid buffer map range");

        // SAFETY: asserts guard this
        unsafe { self.get_range_unchecked_mut(range) }
    }
}
