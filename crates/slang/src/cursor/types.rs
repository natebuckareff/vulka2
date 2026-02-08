use anyhow::{Result, anyhow};
use bytemuck::Pod;

use crate::{CursorLayout, CursorLayoutView, ShaderOffset};

struct ShaderCursor<'a> {
    layout: &'a CursorLayout,
    view: CursorLayoutView,
    object: Box<dyn ShaderObject>,
}

impl<'a> ShaderCursor<'a> {
    fn new(
        layout: &'a CursorLayout,
        view: CursorLayoutView,
        object: Box<dyn ShaderObject>,
    ) -> Self {
        Self {
            layout,
            view,
            object,
        }
    }

    fn field(self, name: &str) -> Result<Self> {
        let view = self
            .layout
            .field(self.view, name)
            .ok_or_else(|| anyhow!("field '{name}' not found or current node is not a struct"))?;

        Ok(Self {
            layout: self.layout,
            view,
            object: self.object,
        })
    }

    fn element(self, index: usize) -> Result<Self> {
        let view = self.layout.element(self.view, index).ok_or_else(|| {
            anyhow!("array element index {index} out of bounds or current node is not an array")
        })?;

        Ok(Self {
            layout: self.layout,
            view,
            object: self.object,
        })
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.object.write(self.view.base, bytes)
    }

    fn write_pod<T: Pod>(&mut self, pod: &T) -> Result<()> {
        self.write_bytes(bytemuck::bytes_of(pod))
    }

    // TODO:
    // - write_bool
    // - write_u8  ...
    // - write_u32 ...
    // - write_f32 ... etc

    fn bind(&mut self, object: Box<dyn ShaderResource>) -> Result<()> {
        let _ = object;
        Err(anyhow!(
            "bind is not implemented yet: requires mutable ShaderParameterBlock access"
        ))
    }

    fn bind_and_resolve(&mut self, object: Box<dyn ShaderResource>) -> Result<ShaderCursor<'a>> {
        let _ = object;
        Err(anyhow!(
            "bind_and_resolve is not implemented yet: requires ShaderResource -> ShaderObject resolution"
        ))
    }
}

// object that supports writing bytes
trait ShaderObject {
    fn as_shader_block(&self) -> Option<&dyn ShaderParameterBlock>;
    fn write(&mut self, offset: ShaderOffset, bytes: &[u8]) -> Result<()>;
}

// object that supports writing descriptors
trait ShaderParameterBlock: ShaderObject {
    fn bind(&mut self, offset: ShaderOffset, object: Box<dyn ShaderResource>) -> Result<()>;
}

// object that supports being written to a ShaderParameterBlock
trait ShaderResource {
    // type Handle;
    // fn handle(&self) -> Self::Handle;
}
