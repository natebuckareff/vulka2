use std::marker::PhantomData;

use anyhow::{Result, anyhow};
use bytemuck::Pod;

use crate::gpu::BufferMap;

pub struct BufferView<T: Pod> {
    map: BufferMap,
    len: u64,
    marker: PhantomData<T>,
}

impl<T: Pod> BufferView<T> {
    pub fn new(map: BufferMap) -> Result<Self> {
        let stride = std::mem::size_of::<T>() as u64;
        if stride == 0 {
            return Err(anyhow!("zero-sized types are invalid"));
        }

        let alignment = std::mem::align_of::<T>() as u64;
        if map.alignment() < alignment {
            return Err(anyhow!("map and view are unaligned"));
        }

        let byte_len = map.len();
        if byte_len % stride != 0 {
            return Err(anyhow!("map size is not a multiple of stride"));
        }

        Ok(Self {
            map,
            len: byte_len / stride,
            marker: PhantomData,
        })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    fn translate_range(&self, range: std::ops::Range<u64>) -> std::ops::Range<u64> {
        let size = std::mem::size_of::<T>() as u64;
        // OVERFLOW: callers only pass ranges with `end <= self.len`. In
        // `new()`, `self.len` is computed as `map.len() / size`, so `self.len *
        // size <= map.len()`.  Since `map.len()` is a `u64`, both `range.start
        // * size` and `range.end * size` are bounded by `self.len * size` and
        // therefore cannot overflow.
        let start = range.start * size;
        let end = range.end * size;
        start..end
    }
}

impl<T: Pod> std::ops::Index<u64> for BufferView<T> {
    type Output = T;

    fn index(&self, index: u64) -> &Self::Output {
        // TODO: doing bounds check twice
        assert!(index < self.len, "buffer view index out-of-bounds");

        let range = self.translate_range(index..index + 1);
        // SAFETY: assert guards this
        bytemuck::from_bytes(unsafe { self.map.get_range_unchecked(range) })
    }
}

impl<T: Pod> std::ops::IndexMut<u64> for BufferView<T> {
    fn index_mut(&mut self, index: u64) -> &mut Self::Output {
        // TODO: doing bounds check twice
        assert!(index < self.len, "buffer view index out-of-bounds");

        let range = self.translate_range(index..index + 1);
        // SAFETY: assert guards this
        bytemuck::from_bytes_mut(unsafe { self.map.get_range_unchecked_mut(range) })
    }
}

impl<T: Pod> std::ops::Index<std::ops::Range<u64>> for BufferView<T> {
    type Output = [T];

    fn index(&self, range: std::ops::Range<u64>) -> &Self::Output {
        assert!(range.start <= self.len(), "buffer view out-of-bounds");
        assert!(range.end <= self.len(), "buffer view out-of-bounds");
        assert!(range.start <= range.end, "invalid buffer view range");

        let range = self.translate_range(range);
        // SAFETY: assert guards this
        bytemuck::cast_slice(unsafe { self.map.get_range_unchecked(range) })
    }
}

impl<T: Pod> std::ops::IndexMut<std::ops::Range<u64>> for BufferView<T> {
    fn index_mut(&mut self, range: std::ops::Range<u64>) -> &mut Self::Output {
        assert!(range.start <= self.len(), "buffer view out-of-bounds");
        assert!(range.end <= self.len(), "buffer view out-of-bounds");
        assert!(range.start <= range.end, "invalid buffer view range");

        let range = self.translate_range(range);
        // SAFETY: assert guards this
        bytemuck::cast_slice_mut(unsafe { self.map.get_range_unchecked_mut(range) })
    }
}
