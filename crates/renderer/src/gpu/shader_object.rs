use std::cell::RefCell;

use anyhow::Result;
use bytemuck::Pod;
use slang::LayoutCursor;

// Cursors wrap objects that support writing bytes or binding resources, for
// some layout
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
    pub fn write<S: ShaderBytes>(&self, value: S) -> Result<()> {
        value.encode(self)
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

// Implemented by the object that supports writing bytes
pub trait ByteWritable {
    fn write_bytes(&mut self, layout: &LayoutCursor, bytes: &[u8]) -> Result<()>;
}

// Implemented by the object that supports binding resources
pub trait ResourceBindable {
    fn bind(&mut self, layout: &LayoutCursor, resource: &ResourceBinding) -> Result<()>;
}

pub enum ResourceBinding {
    UniformBuffer(/* TODO */),
    SampledImage(/* TODO */),
    Sampler(/* TODO */),
    CombinedImageSampler(/* TODO */),
}

// Anything that can be written as bytes to a buffer should implement this
pub trait ShaderBytes {
    fn encode<W: ByteWritable>(&self, cursor: &ShaderCursor<W>) -> Result<()>;
}

impl ShaderBytes for bool {
    fn encode<W: ByteWritable>(&self, cursor: &ShaderCursor<W>) -> Result<()> {
        let value: i32 = if *self { 1 } else { 0 };
        cursor.write_bytes(bytemuck::bytes_of(&value))
    }
}

impl<T: Pod> ShaderBytes for &T {
    fn encode<W: ByteWritable>(&self, cursor: &ShaderCursor<W>) -> Result<()> {
        let bytes = bytemuck::bytes_of(*self);
        cursor.write_bytes(bytes)
    }
}

impl<T: Pod> ShaderBytes for &[T] {
    fn encode<W: ByteWritable>(&self, cursor: &ShaderCursor<W>) -> Result<()> {
        let bytes = bytemuck::cast_slice(*self);
        cursor.write_bytes(bytes)
    }
}
