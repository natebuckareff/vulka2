use std::{cell::RefCell, rc::Rc};

use anyhow::{Context, Ok, Result, anyhow};
use bytemuck::Pod;
use slang::LayoutCursor;

pub struct ShaderCursor<T> {
    layout: LayoutCursor,
    object: Rc<RefCell<T>>,
}

impl<T> Clone for ShaderCursor<T> {
    fn clone(&self) -> Self {
        Self {
            layout: self.layout.clone(),
            object: self.object.clone(),
        }
    }
}

impl<T> ShaderCursor<T> {
    fn new(layout: LayoutCursor, object: Rc<RefCell<T>>) -> Self {
        Self { layout, object }
    }

    fn field(&self, name: &str) -> Result<Self> {
        self.layout
            .field(name)
            .context("field not found")
            .map(|layout| Self {
                layout,
                object: self.object.clone(),
            })
    }

    fn element(&self, index: usize) -> Result<Self> {
        self.layout
            .element(index)
            .context("element index out of bounds")
            .map(|layout| Self {
                layout,
                object: self.object.clone(),
            })
    }
}

impl<T: ShaderObject + ByteWritable> ShaderCursor<T> {
    fn write_pod<P: Pod>(&self, pod: &P) -> Result<()> {
        let mut object = self.object.borrow_mut();
        if object.is_finalized() {
            return Err(anyhow!("cannot write to finalized object"));
        }
        object.write_pod(&self.layout, pod)
    }

    fn write_bytes(&self, bytes: &[u8]) -> Result<()> {
        let mut object = self.object.borrow_mut();
        if object.is_finalized() {
            return Err(anyhow!("cannot write to finalized object"));
        }
        object.write_bytes(&self.layout, bytes)
    }
}

impl<T: ShaderObject + ResourceBindable> ShaderCursor<T> {
    fn bind(&self, resource: &ResourceBinding) -> Result<()> {
        let mut object = self.object.borrow_mut();
        if object.is_finalized() {
            return Err(anyhow!("cannot write to finalized object"));
        }
        object.bind(&self.layout, resource)
    }
}

pub trait ShaderObject {
    fn is_finalized(&self) -> bool;
}

pub trait ByteWritable {
    fn write_pod<P: Pod>(&mut self, layout: &LayoutCursor, pod: &P) -> Result<()>;
    fn write_bytes(&mut self, layout: &LayoutCursor, bytes: &[u8]) -> Result<()>;
}

pub trait ResourceBindable {
    fn bind(&mut self, layout: &LayoutCursor, resource: &ResourceBinding) -> Result<()>;
}

enum ResourceBinding {
    UniformBuffer(/* TODO */),
    StorageBuffer(/* TODO */),
    SampledImage(/* TODO */),
    Sampler(/* TODO */),
    CombinedImageSampler(/* TODO */),
}

/*
TODO delete

struct DescriptorSetWriter {
    is_finalized: bool,
    fake_state: i32,
}

impl ShaderObject for DescriptorSetWriter {
    fn is_finalized(&self) -> bool {
        self.is_finalized
    }
}

impl ByteWritable for DescriptorSetWriter {
    fn write_pod<P: Pod>(&mut self, layout: &LayoutCursor, pod: &P) -> Result<()> {
        todo!()
    }

    fn write_bytes(&mut self, layout: &LayoutCursor, bytes: &[u8]) -> Result<()> {
        self.fake_state += 1;
        Ok(())
    }
}

impl ResourceBindable for DescriptorSetWriter {
    fn bind(&mut self, layout: &LayoutCursor, resource: &ResourceBinding) -> Result<()> {
        self.fake_state += 1;
        Ok(())
    }
}

struct MockDescriptorSet {
    layout: LayoutCursor,
    writer: Rc<RefCell<DescriptorSetWriter>>,
}

impl MockDescriptorSet {
    fn cursor(&mut self) -> ShaderCursor<DescriptorSetWriter> {
        ShaderCursor::new(self.layout.clone(), self.writer.clone())
    }

    fn finish(self) -> MockDescriptorSetToken {
        let writer = self.writer;
        writer.borrow_mut().is_finalized = true;
        MockDescriptorSetToken { writer }
    }
}

struct MockDescriptorSetToken {
    writer: Rc<RefCell<DescriptorSetWriter>>, // TODO
}

fn test(mut set: MockDescriptorSet, texture: (), params: ()) -> Result<()> {
    let mut cursor_x = set.cursor().field("x")?;
    let mut cursor_y = set.cursor().field("y")?;

    for _ in 0..10 {
        cursor_x.write_pod(&())?;
        cursor_y.write_pod(&())?;
    }

    set.finish();

    todo!()
    //
}
*/
