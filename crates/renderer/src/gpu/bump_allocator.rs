use anyhow::Result;
use num_traits::{PrimInt, Unsigned};
use vulkanalia_vma as vma;

use crate::gpu::{AllocHandle, AllocatorId, BufferAllocator, BufferSpan, BufferStorage, Range};

pub struct BumpAllocator {
    id: AllocatorId,
    backing: BufferSpan,
    capacity: u64,
    offset: u64,
}

impl BumpAllocator {
    pub fn new(backing: BufferSpan) -> Result<Self> {
        backing.buffer().check_flags(
            vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vma::AllocationCreateFlags::MAPPED,
        )?;
        let capacity = backing.range().size().try_into()?;
        let offset = backing.range().start();
        Ok(Self {
            id: AllocatorId::new(),
            backing,
            capacity,
            offset,
        })
    }
}

impl BufferStorage for BumpAllocator {
    fn id(&self) -> AllocatorId {
        self.id
    }

    fn backing(&self) -> &BufferSpan {
        &self.backing
    }

    fn free(self) -> BufferSpan {
        self.backing
    }
}

impl BufferAllocator for BumpAllocator {
    fn len(&self) -> u64 {
        self.offset
    }

    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn acquire(&mut self, size: u64, align: Option<u64>) -> Result<Option<BufferSpan>> {
        let align = align.unwrap_or(1);
        let start = align_up(self.offset, align);
        let range = Range::sized(start, size)?;
        if !self.backing.range().fits(range) {
            return Ok(None);
        }
        let handle = AllocHandle::dummy();
        let span = BufferSpan::from_allocator(self, handle, range);
        self.offset = range.end();
        Ok(Some(span))
    }
}

pub(crate) fn align_up<T>(value: T, align: T) -> T
where
    T: PrimInt + Unsigned,
{
    debug_assert!(align != T::zero());
    debug_assert!((align & (align - T::one())) == T::zero());

    let mask = align - T::one();
    (value + mask) & !mask
}
