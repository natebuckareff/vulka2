use std::cell::RefCell;

use anyhow::{Context, Result, anyhow};
use slang::LayoutCursor;
use vulkanalia::vk;

use crate::gpu::{
    BufferToken, BufferWriter, ByteWritable, DescriptorSet, DescriptorSetHandle, ResourceBindable,
    ResourceBinding, RetireToken, ShaderCursor,
};

// TODO: rename to ParameterObject?
pub struct ParameterBlock<Handle: Copy> {
    layout: LayoutCursor,
    writer: RefCell<ParameterWriter<Handle>>,
}

impl<Handle: Copy> ParameterBlock<Handle> {
    pub fn new(writer: ParameterWriter<Handle>) -> Self {
        Self {
            layout: writer.set.set_layout().layout().rebase(),
            writer: RefCell::new(writer),
        }
    }

    pub fn cursor(&self) -> ShaderCursor<'_, ParameterWriter<Handle>> {
        ShaderCursor::new(self.layout.clone(), &self.writer)
    }

    pub fn finish(self) -> Result<ParameterToken<Handle>> {
        self.writer.into_inner().finish()
    }
}

pub struct ParameterWriter<Handle: Copy> {
    set: DescriptorSet,
    ubo: Option<BufferWriter<Handle>>,
}

impl<Handle: Copy> ParameterWriter<Handle> {
    pub fn new(set: DescriptorSet, ubo: Option<BufferWriter<Handle>>) -> Self {
        Self { set, ubo }
    }

    pub fn finish(self) -> Result<ParameterToken<Handle>> {
        let handle = *self.set.handle();
        let ubo = self.ubo.map(|writer| writer.finish()).transpose()?;
        Ok(ParameterToken::new(handle, ubo))
    }
}

impl<Handle: Copy> ResourceBindable for ParameterWriter<Handle> {
    fn bind(&mut self, layout: &slang::LayoutCursor, resource: &ResourceBinding) -> Result<()> {
        let parameter_block_layout = self.set.set_layout().layout().parameter_block_layout()?;
        let binding_range = layout.offset().binding_range;
        let binding = &parameter_block_layout
            .find_binding_range(binding_range)
            .context("binding range out-of-bounds")?
            .descriptor;

        use ResourceBinding::*;
        match (resource, binding.descriptor_type) {
            (UniformBuffer(), vk::DescriptorType::UNIFORM_BUFFER) => {
                todo!();
            }
            (SampledImage(), vk::DescriptorType::SAMPLED_IMAGE) => {
                todo!()
            }
            (Sampler(), vk::DescriptorType::SAMPLER) => {
                todo!()
            }
            (CombinedImageSampler(), vk::DescriptorType::COMBINED_IMAGE_SAMPLER) => {
                todo!()
            }
            _ => {}
        };

        todo!()
    }
}

impl<Handle: Copy> ByteWritable for ParameterWriter<Handle> {
    fn write_bytes(&mut self, layout: &slang::LayoutCursor, bytes: &[u8]) -> Result<()> {
        let Some(ubo) = &mut self.ubo else {
            return Err(anyhow!("implicit parameter block ubo not found"));
        };
        ubo.write_bytes(layout, bytes)
    }
}

// TODO: all of these token wrappers around RetireToken, including BufferToken,
// need some higher-level API for CommandBuffer to use
pub struct ParameterToken<Handle: Copy> {
    retire: RetireToken<DescriptorSetHandle>,
    ubo: Option<BufferToken<Handle>>,
}

impl<Handle: Copy> ParameterToken<Handle> {
    pub fn new(handle: DescriptorSetHandle, ubo: Option<BufferToken<Handle>>) -> Self {
        Self {
            retire: RetireToken::new(handle),
            ubo,
        }
    }

    pub fn split(self) -> (DescriptorSetToken, Option<BufferToken<Handle>>) {
        let token = DescriptorSetToken {
            retire: self.retire,
        };
        (token, self.ubo)
    }
}

pub struct DescriptorSetToken {
    retire: RetireToken<DescriptorSetHandle>,
}

impl DescriptorSetToken {
    // TODO: rename parts() -> into_parts() in this codebase
    pub fn into_retire(self) -> RetireToken<DescriptorSetHandle> {
        self.retire
    }
}
