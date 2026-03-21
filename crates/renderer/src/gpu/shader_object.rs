use std::{cell::RefCell, rc::Rc};

use anyhow::{Result, anyhow};
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
    pub fn new(layout: LayoutCursor, object: Rc<RefCell<T>>) -> Self {
        Self { layout, object }
    }

    pub fn field(&self, name: &str) -> Result<Self> {
        self.layout.field(name).map(|layout| Self {
            layout,
            object: self.object.clone(),
        })
    }

    pub fn index(&self, index: usize) -> Result<Self> {
        self.layout.index(index).map(|layout| Self {
            layout,
            object: self.object.clone(),
        })
    }
}

impl<T: ShaderObject + ByteWritable> ShaderCursor<T> {
    pub fn write_pod<P: Pod>(&self, pod: &P) -> Result<()> {
        let mut object = self.object.borrow_mut();
        if object.is_finished() {
            return Err(anyhow!("cannot write to finalized object"));
        }
        object.write_pod(&self.layout, pod)
    }

    pub fn write_bytes(&self, bytes: &[u8]) -> Result<()> {
        let mut object = self.object.borrow_mut();
        if object.is_finished() {
            return Err(anyhow!("cannot write to finalized object"));
        }
        object.write_bytes(&self.layout, bytes)
    }
}

impl<T: ShaderObject + ResourceBindable> ShaderCursor<T> {
    pub fn bind(&self, resource: &ResourceBinding) -> Result<()> {
        let mut object = self.object.borrow_mut();
        if object.is_finished() {
            return Err(anyhow!("cannot write to finalized object"));
        }
        object.bind(&self.layout, resource)
    }
}

pub trait ShaderObject {
    fn is_finished(&self) -> bool;
}

pub trait ByteWritable {
    fn write_pod<P: Pod>(&mut self, layout: &LayoutCursor, pod: &P) -> Result<()>;
    fn write_bytes(&mut self, layout: &LayoutCursor, bytes: &[u8]) -> Result<()>;
}

pub trait ResourceBindable {
    fn bind(&mut self, layout: &LayoutCursor, resource: &ResourceBinding) -> Result<()>;
}

pub enum ResourceBinding {
    UniformBuffer(/* TODO */),
    StorageBuffer(/* TODO */),
    SampledImage(/* TODO */),
    Sampler(/* TODO */),
    CombinedImageSampler(/* TODO */),
}
