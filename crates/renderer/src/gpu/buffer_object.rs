use std::cell::RefCell;

use anyhow::Result;
use slang::LayoutCursor;

use crate::gpu::{BufferToken, BufferWriter, ShaderCursor};

pub struct BufferObject<Handle: Copy> {
    layout: LayoutCursor,
    writer: RefCell<BufferWriter<Handle>>,
}

impl<Handle: Copy> BufferObject<Handle> {
    pub fn new(layout: &LayoutCursor, writer: BufferWriter<Handle>) -> Self {
        Self {
            layout: layout.rebase(),
            writer: RefCell::new(writer),
        }
    }

    pub fn cursor(&self) -> ShaderCursor<'_, BufferWriter<Handle>> {
        ShaderCursor::new(self.layout.clone(), &self.writer)
    }

    pub fn finish(self) -> Result<BufferToken<Handle>> {
        // TODO: this feels like a design issue / knot
        let writer = self.writer.into_inner();
        writer.finish()
    }
}
