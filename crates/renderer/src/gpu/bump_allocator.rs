use anyhow::Result;

use crate::gpu::{AllocHandle, AllocatorId, BufferAllocator, BufferStorage, Range, StorageSpan};

pub struct BumpAllocator<T: StorageSpan> {
    id: AllocatorId,
    storage: T,
    capacity: u64,
    offset: u64,
}

impl<T: StorageSpan> BumpAllocator<T> {
    pub fn new(storage: T) -> Result<Self> {
        let capacity = storage.span().range().size().try_into()?;
        let offset = storage.span().range().start();
        Ok(Self {
            id: AllocatorId::new(),
            storage,
            capacity,
            offset,
        })
    }
}

impl<T: StorageSpan> BufferStorage for BumpAllocator<T> {
    type Storage = T;

    fn id(&self) -> AllocatorId {
        self.id
    }

    fn storage(&self) -> &Self::Storage {
        &self.storage
    }

    fn free(self) -> Self::Storage {
        self.storage
    }
}

impl<T: StorageSpan> BufferAllocator for BumpAllocator<T> {
    fn len(&self) -> u64 {
        self.offset
    }

    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn acquire(&mut self, size: u64, align: Option<u64>) -> Result<Option<T>> {
        let align = align.unwrap_or(1);
        let start = self.storage.align(self.offset, align);
        let range = Range::sized(start, size)?;
        if !self.storage.span().range().fits(range) {
            return Ok(None);
        }
        let handle = AllocHandle::dummy();
        let span = self.storage.suballocate(self, handle, range)?;
        self.offset = range.end();
        Ok(Some(span))
    }
}
