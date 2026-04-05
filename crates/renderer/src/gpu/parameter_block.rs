use std::cell::RefCell;

use anyhow::{Result, anyhow};
use bytemuck::Pod;

use crate::gpu::{
    BufferObject, BufferSpan, BufferToken, BufferWriter, DescriptorSet, DescriptorSetHandle,
    FrameToken, LaneKey, RetireToken, ShaderDescriptor,
};

pub struct ParameterBlock {
    layout: slang::LayoutCursor,
    parameter_writer: RefCell<ParameterWriter>,
    ubo_writer: Option<RefCell<BufferWriter>>,
}

impl ParameterBlock {
    pub fn new(parameter_writer: ParameterWriter, ubo_writer: Option<BufferWriter>) -> Self {
        Self {
            layout: parameter_writer.set.set_layout().layout().rebase(),
            parameter_writer: RefCell::new(parameter_writer),
            ubo_writer: ubo_writer.map(RefCell::new),
        }
    }

    pub fn cursor(&self) -> ParameterCursor<'_> {
        ParameterCursor {
            layout: self.layout.clone(),
            parameter_writer: &self.parameter_writer,
            ubo_writer: self.ubo_writer.as_ref(),
        }
    }

    pub fn finish(self) -> Result<ParameterToken> {
        let set = self.parameter_writer.into_inner().finish();
        let ubo_token = self
            .ubo_writer
            .map(|u| u.into_inner().finish())
            .transpose()?;
        let retire = RetireToken::new(*set.handle());
        let set_token = DescriptorSetToken {
            set: Box::new(set),
            retire,
        };
        Ok(ParameterToken {
            set_token,
            ubo_token,
        })
    }
}

// TODO: how are dynamic offsets bound? can figure that out when working on
// descriptor set binding command
pub struct ParameterToken {
    set_token: DescriptorSetToken,
    ubo_token: Option<BufferToken>,
}

impl ParameterToken {
    pub fn set_token(&self) -> &DescriptorSetToken {
        &self.set_token
    }

    pub fn ubo_token(&self) -> Option<&BufferToken> {
        self.ubo_token.as_ref()
    }

    // TODO: trait?
    pub fn touch(&mut self, key: LaneKey, frame: &FrameToken) {
        self.set_token.touch(key, frame);
        self.ubo_token.as_mut().map(|token| token.touch(key, frame));
    }

    pub fn split(self) -> (DescriptorSetToken, Option<BufferToken>) {
        (self.set_token, self.ubo_token)
    }
}

pub struct DescriptorSetToken {
    set: Box<DescriptorSet>,
    retire: RetireToken<DescriptorSetHandle>,
}

impl DescriptorSetToken {
    pub fn set(&self) -> &DescriptorSet {
        &self.set
    }

    // TODO: trait?
    pub fn touch(&mut self, key: LaneKey, frame: &FrameToken) {
        self.retire.touch(key, frame);
    }

    pub fn into_parts(self) -> (Box<DescriptorSet>, RetireToken<DescriptorSetHandle>) {
        (self.set, self.retire)
    }
}

pub struct ParameterWriter {
    set: DescriptorSet,
}

impl ParameterWriter {
    pub fn new(set: DescriptorSet) -> Self {
        Self { set }
    }

    fn write_descriptor<T: ShaderDescriptor>(
        &mut self,
        layout: &slang::LayoutCursor,
        value: T,
    ) -> Result<()> {
        value.encode_into(layout, &mut self.set)
    }

    fn finish(self) -> DescriptorSet {
        self.set
    }
}

pub struct ParameterCursor<'obj> {
    layout: slang::LayoutCursor,
    parameter_writer: &'obj RefCell<ParameterWriter>,
    ubo_writer: Option<&'obj RefCell<BufferWriter>>,
}

impl<'obj> ParameterCursor<'obj> {
    pub fn field(&self, name: &str) -> Result<Self> {
        Ok(Self {
            layout: self.layout.field(name)?,
            parameter_writer: self.parameter_writer,
            ubo_writer: self.ubo_writer,
        })
    }

    pub fn index(&self, index: usize) -> Result<Self> {
        Ok(Self {
            layout: self.layout.index(index)?,
            parameter_writer: self.parameter_writer,
            ubo_writer: self.ubo_writer,
        })
    }

    // returns handle to non-implicit ubo binding
    pub fn uniform<'r>(&self, span: BufferSpan) -> BufferObject {
        span.object(&self.layout)
    }

    // writes into implicit ubo
    pub fn set<T: Pod>(&self, value: T) -> Result<()> {
        self.write(&value)
    }

    // writes into implicit ubo
    pub fn write<T: Pod>(&self, value: &T) -> Result<()> {
        let Some(mut writer) = self.ubo_writer.map(RefCell::borrow_mut) else {
            return Err(anyhow!("parameter block has no implicit uniform buffer"));
        };
        writer.write(&self.layout, value)
    }

    // writes into descriptor
    pub fn write_descriptor<T: ShaderDescriptor>(&self, value: T) -> Result<()> {
        let mut writer = self.parameter_writer.borrow_mut();
        // OPTIMIZE: should bulk write all the descriptors in one go instead of
        // one-by-one
        writer.write_descriptor(&self.layout, value)
    }
}
