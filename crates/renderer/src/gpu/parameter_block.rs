use std::{cell::RefCell, rc::Rc};

use anyhow::Result;

use crate::gpu::{
    ByteWritable, DescriptorSet, FinishedDescriptorSet, ResourceBindable, ResourceBinding,
    ShaderCursor,
};

pub type BufferRef = (); // TODO

pub struct ParameterBlock {
    set: DescriptorSet,
    ubo: Option<BufferRef>,
    writer: Rc<RefCell<ParameterWriter>>,
}

impl ParameterBlock {
    pub(crate) fn new(set: DescriptorSet, ubo: Option<BufferRef>) -> Self {
        let writer = Rc::new(RefCell::new(ParameterWriter::new()));
        Self { set, ubo, writer }
    }

    pub fn cursor(&self) -> ShaderCursor<'_, ParameterWriter> {
        todo!()
    }

    pub fn finish(self) -> FinishedDescriptorSet {
        assert_eq!(Rc::strong_count(&self.writer), 1);
        self.set.finish()
    }
}

pub struct ParameterWriter {
    // TODO
}

impl ParameterWriter {
    fn new() -> Self {
        Self {}
    }
}

impl ResourceBindable for ParameterWriter {
    fn bind(
        &mut self,
        layout: &slang::LayoutCursor,
        resource: &ResourceBinding,
    ) -> anyhow::Result<()> {
        todo!()
    }
}

impl ByteWritable for ParameterWriter {
    fn write_pod<P: bytemuck::Pod>(
        &mut self,
        layout: &slang::LayoutCursor,
        pod: &P,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn write_bytes(&mut self, layout: &slang::LayoutCursor, bytes: &[u8]) -> anyhow::Result<()> {
        todo!()
    }
}

/*
fn test(pb: ParameterBlock) -> Result<()> {
    let cursor = pb.cursor();

    let cursor_foo = cursor.field("foo")?;
    let cursor_bar = cursor.field("bar")?;

    for _ in 0..10 {
        cursor_foo.write_bytes(&[])?;
        cursor_bar.write_bytes(&[])?;
    }

    let x = pb.finish();

    Ok(())
}
*/
