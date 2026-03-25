use anyhow::Result;

use crate::gpu::{AllocId, BufferSpan, BufferToken};

pub trait BufferAllocator: BufferBlock {
    fn deallocate(&mut self, span: BufferSpan<Self::Handle>) -> Result<()>;
}

pub trait BufferBlock {
    type Storage: Copy;
    type Handle: Copy;

    fn id(&self) -> AllocId;

    fn acquire(
        &mut self,
        size: u64,
        align: Option<u64>,
    ) -> Result<Option<BufferSpan<Self::Handle>>>;

    fn free(self) -> BufferSpan<Self::Storage>;

    fn owns_span(&self, span: BufferSpan<Self::Handle>) -> bool {
        span.id() == Some(self.id())
    }

    fn owns_token(&self, token: &BufferToken<Self::Handle>) -> bool {
        token.id() == Some(self.id())
    }
}
