use anyhow::Result;
use num_traits::{PrimInt, Unsigned};

use crate::gpu::{AllocHandle, BufferAllocator, BufferSpan, MapSpan, Range};

pub trait StorageSpan: Sized {
    fn align(&self, offset: u64, alignment: u64) -> u64;

    fn align_relative(&self, offset: u64, alignment: u64) -> u64 {
        debug_assert!(offset < self.span().range().size());
        let start = self.span().range().start();
        // OVERFLOW: Range guarantees that `start + size == end` for a given
        // span, therefore so long as offset is span-range-relative then this
        // will not overflow
        let absolute = start + offset;
        let aligned = self.align(absolute, alignment);
        aligned - start
    }

    fn suballocate(
        &self,
        allocator: &impl BufferAllocator,
        handle: AllocHandle,
        range: Range,
    ) -> Result<Self>;

    fn span(&self) -> &BufferSpan;

    fn into_span(self) -> BufferSpan;
}

impl StorageSpan for BufferSpan {
    fn align(&self, offset: u64, alignment: u64) -> u64 {
        align_up(offset, alignment)
    }

    fn suballocate(
        &self,
        allocator: &impl BufferAllocator,
        handle: AllocHandle,
        range: Range,
    ) -> Result<Self> {
        Ok(BufferSpan::from_allocator(allocator, handle, range))
    }

    fn span(&self) -> &BufferSpan {
        self
    }

    fn into_span(self) -> BufferSpan {
        self
    }
}

impl StorageSpan for MapSpan {
    fn align(&self, offset: u64, alignment: u64) -> u64 {
        let base = self.base_pointer();
        let effective = base + offset;
        let aligned = align_up(effective, alignment);
        aligned - base
    }

    fn suballocate(
        &self,
        allocator: &impl BufferAllocator,
        handle: AllocHandle,
        range: Range,
    ) -> Result<Self> {
        let span = BufferSpan::from_allocator(allocator, handle, range);
        MapSpan::new(span)
    }

    fn span(&self) -> &BufferSpan {
        self.span()
    }

    fn into_span(self) -> BufferSpan {
        self.into_span()
    }
}

fn align_up<T>(value: T, align: T) -> T
where
    T: PrimInt + Unsigned,
{
    debug_assert!(align != T::zero());
    debug_assert!((align & (align - T::one())) == T::zero());

    let mask = align - T::one();
    (value + mask) & !mask
}
