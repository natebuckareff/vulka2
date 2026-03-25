use anyhow::Result;

use crate::gpu::BufferSpan;

pub trait BufferAllocator: BufferBlock {
    fn deallocate(&mut self, span: Self::Span) -> Result<()>;
}

pub trait BufferBlock {
    type Storage: Copy;
    type Handle: Copy;
    type Span = BufferSpan<Self::Handle>;
    fn owns(&self, span: Self::Span) -> bool;
    fn acquire(&mut self, size: u64, align: Option<u64>) -> Result<Option<Self::Span>>;
    fn free(self) -> BufferSpan<Self::Storage>;
}
