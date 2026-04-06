use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::Result;

use crate::gpu::{BufferSpan, StorageSpan};

pub trait BackingAllocator: BufferAllocator {
    fn deallocate(&mut self, span: BufferSpan) -> Result<()>;
}

pub trait BufferStorage {
    type Storage: StorageSpan;
    fn id(&self) -> AllocatorId;
    fn storage(&self) -> &Self::Storage;
    fn free(self) -> Self::Storage;
}

pub trait BufferAllocator: BufferStorage {
    fn len(&self) -> u64;
    fn capacity(&self) -> u64;
    // TODO: rename `allocate`
    fn acquire(&mut self, size: u64, align: Option<u64>) -> Result<Option<Self::Storage>>;
}

pub trait BlockAllocator: BufferStorage {
    fn len(&self) -> u64;
    fn capacity(&self) -> u64;
    fn block_size(&self) -> u64;
    fn block_alignment(&self) -> u64;
    // TODO: rename `allocate`
    fn acquire(&mut self) -> Result<Option<BufferSpan>>;
}

// TODO: Telemetry events system to monitor span ownership and raise errors in
// debug builds. Have a global context that all object can use to emit telemetry
// events and implement decoupled aggregator logic. First use-case is raising an
// error if a BufferSpan is never deallocated

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AllocatorId(u32);

impl AllocatorId {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    pub(crate) fn buffer() -> Self {
        Self(u32::MAX)
    }

    pub fn is_buffer(&self) -> bool {
        self.0 == u32::MAX
    }
}
