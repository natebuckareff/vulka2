// NAMING CONVENTIONS:
// - SlangThing     APIs for compiling, linking, and reflecting slang shaders
// - ShaderThing    APIs for dealing with the write boundary of shaders
// - ThingObject    APIs for dealing with opaque GPU memory resources
// - GpuThing       safe, rust wrappers for vulkan objects

use anyhow::Result;
use bytemuck::Pod;

use crate::{CursorLayout, SlangShaderStage};

// ~

struct ShaderOffset {
    // TODO
}

struct ShaderCursor {
    object: Option<Box<dyn ShaderObject>>,
    offset: ShaderOffset,
}

impl ShaderCursor {
    fn new(layout: CursorLayout, object: Box<dyn ShaderObject>) -> Self {
        todo!()
    }

    fn field(self, name: &str) -> Result<Self> {
        todo!()
    }

    fn element(self, index: usize) -> Result<Self> {
        todo!()
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        todo!()
    }

    fn write_pod<T: Pod>(&mut self, pod: &T) -> Result<()> {
        todo!()
    }

    // TODO: etc

    fn bind(&mut self, object: Box<dyn ShaderResource>) -> Result<()> {
        todo!()
    }

    fn bind_and_resolve(&mut self, object: Box<dyn ShaderResource>) -> Result<ShaderCursor> {
        todo!()
    }
}

trait ShaderObject {
    fn as_shader_block(&self) -> Option<&dyn ShaderParameterBlock>;
    fn write(&mut self, offset: ShaderOffset, bytes: &[u8]) -> Result<()>;
}

trait ShaderParameterBlock: ShaderObject {
    fn bind(&mut self, offset: ShaderOffset, object: Box<dyn ShaderResource>) -> Result<()>;
}

trait ShaderResource {
    // type Handle;
    // fn handle(&self) -> Self::Handle;
}

/*
Objects:
- ByteObject           // CPU bytes
- ParameterBlockObject // descriptor set with BufferObject for implicit UBO
- BufferObject         // VkBuffer
- TextureObject        // VkImage?
*/
