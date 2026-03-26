use anyhow::Result;
use vulkanalia_vma as vma;

use crate::gpu::{AllocId, BufferBlock, BufferSpan, Range};

pub struct BumpAllocator<Storage: Copy> {
    id: AllocId,
    storage: BufferSpan<Storage>,
    offset: u64,
}

impl<Storage: Copy> BumpAllocator<Storage> {
    pub fn new(storage: BufferSpan<Storage>) -> Result<Self> {
        storage.buffer().check_flags(
            vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vma::AllocationCreateFlags::MAPPED,
        )?;
        let offset = storage.range().start();
        Ok(Self {
            id: AllocId::new(),
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

impl<Storage: Copy> BufferBlock for BumpAllocator<Storage> {
    type Storage = Storage;
    type Handle = ();

    fn id(&self) -> super::AllocId {
        self.id
    }

    fn acquire(&mut self, size: u64, align: Option<u64>) -> Result<Option<BufferSpan<()>>> {
        let align = align.unwrap_or(1);
        let start = align_up(self.offset, align);
        let span_range = Range::sized(start, size)?;
        if !self.storage.range().fits(span_range) {
            return Ok(None);
        }
        let buffer = self.storage.buffer().clone();
        let span = BufferSpan::new(Some(self.id), buffer, (), span_range);
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
