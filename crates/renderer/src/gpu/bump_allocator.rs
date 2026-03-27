use anyhow::Result;
use vulkanalia_vma as vma;

use crate::gpu::{AllocatorId, BufferAllocator, BufferSpan, Range};

pub struct BumpAllocator<Storage: Copy> {
    id: AllocatorId,
    storage: BufferSpan<Storage>,
    offset: u64,
}

impl<Storage: Copy> BumpAllocator<Storage> {
    pub fn new(storage: BufferSpan<Storage>) -> Result<Self> {
        storage.allocation().allocator().buffer().check_flags(
            vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vma::AllocationCreateFlags::MAPPED,
        )?;
        let offset = storage.range().start();
        Ok(Self {
            id: AllocatorId::new(),
            storage,
            offset,
        })
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    // TODO: is "capacity" the right name here?
    pub fn capacity(&self) -> u64 {
        self.storage.range().end().saturating_sub(self.offset)
    }
}

impl<Storage: Copy> BufferAllocator for BumpAllocator<Storage> {
    type Storage = Storage;
    type Handle = ();

    fn storage(&self) -> &BufferSpan<Self::Storage> {
        &self.storage
    }

    fn id(&self) -> AllocatorId {
        self.id
    }

    fn acquire(&mut self, size: u64, align: Option<u64>) -> Result<Option<BufferSpan<()>>> {
        let align = align.unwrap_or(1);
        let start = align_up(self.offset, align);
        let span_range = Range::sized(start, size)?;
        if !self.storage.range().fits(span_range) {
            return Ok(None);
        }
        let span = BufferSpan::from_allocator(self, (), span_range);
        self.offset = span_range.end();
        Ok(Some(span))
    }

    fn free(self) -> BufferSpan<Self::Storage> {
        self.storage
    }
}

// TODO: move to a util file
pub(crate) fn align_up(value: u64, align: u64) -> u64 {
    debug_assert!(align.is_power_of_two());
    (value + (align - 1)) & !(align - 1)
}
