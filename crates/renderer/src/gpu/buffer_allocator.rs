use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;

use crate::gpu::{Buffer, BufferSpan, BufferToken};

pub trait BackingAllocator: BufferAllocator {
    fn deallocate(&mut self, span: BufferSpan<Self::Handle>) -> Result<()>;
}

pub trait BufferAllocator {
    type Storage: Copy;
    type Handle: Copy;

    fn storage(&self) -> &BufferSpan<Self::Storage>;
    fn id(&self) -> AllocatorId;

    fn buffer(&self) -> &Arc<Buffer> {
        self.storage().allocation().allocator().buffer()
    }

    fn base(&self) -> u64 {
        self.storage().range().start()
    }

    fn acquire(
        &mut self,
        size: u64,
        align: Option<u64>,
    ) -> Result<Option<BufferSpan<Self::Handle>>>;

    fn free(self) -> BufferSpan<Self::Storage>;

    fn owns_span(&self, span: BufferSpan<Self::Handle>) -> bool {
        span.allocation().allocator().id() == Some(self.id())
    }

    fn owns_token(&self, token: &BufferToken<Self::Handle>) -> bool {
        token.allocator().id() == Some(self.id())
    }
}

// TODO: Telemetry events system to monitor span ownership and raise errors in
// debug builds. Have a global context that all object can use to emit telemetry
// events and implement decoupled aggregator logic. First use-case is raising an
// error if a BufferSpan is never deallocated

pub struct AllocatorInfo {
    buffer: Arc<Buffer>,
    id: Option<AllocatorId>,
    base: u64,
}

impl AllocatorInfo {
    pub fn from_buffer(buffer: Buffer) -> Self {
        Self {
            buffer: Arc::new(buffer),
            id: None,
            base: 0,
        }
    }

    pub fn from_allocator(allocator: &impl BufferAllocator) -> Self {
        let info = allocator.storage().allocation().allocator();
        Self {
            buffer: info.buffer().clone(),
            id: Some(allocator.id()),
            base: allocator.base(),
        }
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    pub fn id(&self) -> Option<AllocatorId> {
        self.id
    }

    pub fn base(&self) -> u64 {
        self.base
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AllocatorId(u64);

impl AllocatorId {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}
