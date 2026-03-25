use std::cell::RefCell;

use anyhow::Result;
use bytemuck::Pod;
use slang::LayoutCursor;

pub struct ShaderCursor<'a, T> {
    layout: LayoutCursor,
    object: &'a RefCell<T>,
}

impl<'a, T> Clone for ShaderCursor<'a, T> {
    fn clone(&self) -> Self {
        Self {
            layout: self.layout.clone(),
            object: self.object,
        }
    }
}

impl<'a, T> ShaderCursor<'a, T> {
    pub fn new(layout: LayoutCursor, object: &'a RefCell<T>) -> Self {
        Self { layout, object }
    }

    pub fn field(&self, name: &str) -> Result<Self> {
        self.layout.field(name).map(|layout| Self {
            layout,
            object: self.object,
        })
    }

    pub fn index(&self, index: usize) -> Result<Self> {
        self.layout.index(index).map(|layout| Self {
            layout,
            object: self.object,
        })
    }
}

impl<'a, T: ByteWritable> ShaderCursor<'a, T> {
    pub fn write_pod<P: Pod>(&self, pod: &P) -> Result<()> {
        self.object.borrow_mut().write_pod(&self.layout, pod)
    }

    pub fn write_slice<P: Pod>(&self, slice: &[P]) -> Result<()> {
        self.object.borrow_mut().write_slice(&self.layout, slice)
    }

    pub fn write_bytes(&self, bytes: &[u8]) -> Result<()> {
        self.object.borrow_mut().write_bytes(&self.layout, bytes)
    }
}

impl<'a, T: ResourceBindable> ShaderCursor<'a, T> {
    pub fn bind(&self, resource: &ResourceBinding) -> Result<()> {
        self.object.borrow_mut().bind(&self.layout, resource)
    }
}

pub trait ByteWritable {
    fn write_pod<T: Pod>(&mut self, layout: &LayoutCursor, value: &T) -> Result<()>;
    fn write_slice<T: Pod>(&mut self, layout: &LayoutCursor, slice: &[T]) -> Result<()>;
    fn write_bytes(&mut self, layout: &LayoutCursor, bytes: &[u8]) -> Result<()>;
}

pub trait ResourceBindable {
    fn bind(&mut self, layout: &LayoutCursor, resource: &ResourceBinding) -> Result<()>;
}

pub enum ResourceBinding {
    UniformBuffer(/* TODO */),
    SampledImage(/* TODO */),
    Sampler(/* TODO */),
    CombinedImageSampler(/* TODO */),
}
