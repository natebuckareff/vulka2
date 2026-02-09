use std::any::Any;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use slang::{
    DescriptorBinding as SlangDescriptorBinding, DescriptorClass, DescriptorProfile,
    DescriptorSet as SlangDescriptorSet, ElementCount, ResourceDescriptor, ShaderObject,
    ShaderOffset, ShaderParameterBlock,
};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;

use super::{GpuDevice, RendererWriteCtx};

#[derive(Clone, Copy)]
pub(crate) enum VkDescriptorValue {
    UniformBuffer(vk::DescriptorBufferInfo),
    StorageBuffer(vk::DescriptorBufferInfo),
    Sampler(vk::DescriptorImageInfo),
    SampledImage(vk::DescriptorImageInfo),
}

impl VkDescriptorValue {
    pub(crate) fn descriptor_type(self) -> vk::DescriptorType {
        match self {
            Self::UniformBuffer(_) => vk::DescriptorType::UNIFORM_BUFFER,
            Self::StorageBuffer(_) => vk::DescriptorType::STORAGE_BUFFER,
            Self::Sampler(_) => vk::DescriptorType::SAMPLER,
            Self::SampledImage(_) => vk::DescriptorType::SAMPLED_IMAGE,
        }
    }

    pub(crate) fn profile(self) -> DescriptorProfile {
        let class = match self {
            Self::UniformBuffer(_) => DescriptorClass::UniformBuffer,
            Self::StorageBuffer(_) => DescriptorClass::StorageBuffer,
            Self::Sampler(_) => DescriptorClass::Sampler,
            Self::SampledImage(_) => DescriptorClass::SampledImage,
        };
        DescriptorProfile {
            class,
            writable: false,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct VkResourceDescriptor {
    pub(crate) value: VkDescriptorValue,
}

impl ResourceDescriptor for VkResourceDescriptor {
    fn profile(&self) -> DescriptorProfile {
        self.value.profile()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone)]
pub struct ParameterObject {
    gpu_device: Arc<GpuDevice>,
    descriptor_set: vk::DescriptorSet,
    descriptor_layout: SlangDescriptorSet,
}

impl ParameterObject {
    pub fn new(
        gpu_device: Arc<GpuDevice>,
        descriptor_set: vk::DescriptorSet,
        descriptor_layout: SlangDescriptorSet,
    ) -> Self {
        Self {
            gpu_device,
            descriptor_set,
            descriptor_layout,
        }
    }

    fn resolve_binding(&self, offset: ShaderOffset) -> Result<&SlangDescriptorBinding> {
        if let Some(binding_range) = self
            .descriptor_layout
            .find_binding_range(offset.binding_range)
        {
            return Ok(&binding_range.descriptor);
        }

        if offset.binding_range == 0 {
            if let Some(binding) = self.descriptor_layout.implicit_ubo.as_ref() {
                return Ok(binding);
            }
        }

        Err(anyhow!(
            "binding range {} not found in parameter block set {:?}",
            offset.binding_range,
            self.descriptor_layout.set
        ))
    }
}

impl<'a> ShaderObject<RendererWriteCtx<'a>> for ParameterObject {
    fn as_shader_block(
        &mut self,
    ) -> Option<&mut dyn ShaderParameterBlock<RendererWriteCtx<'a>>> {
        Some(self)
    }

    fn write(
        &mut self,
        _ctx: &mut RendererWriteCtx<'a>,
        _offset: ShaderOffset,
        _bytes: &[u8],
    ) -> Result<()> {
        Err(anyhow!("parameter objects do not support byte writes"))
    }
}

impl<'a> ShaderParameterBlock<RendererWriteCtx<'a>> for ParameterObject {
    fn bind(
        &mut self,
        _ctx: &mut RendererWriteCtx<'a>,
        offset: ShaderOffset,
        descriptor: Box<dyn ResourceDescriptor>,
    ) -> Result<()> {
        let binding = self.resolve_binding(offset)?;
        let count = match binding.count {
            ElementCount::Bounded(count) => count,
            ElementCount::Runtime => {
                return Err(anyhow!(
                    "runtime-sized descriptor arrays are not supported in test renderer"
                ));
            }
        };

        if offset.array_index >= count {
            return Err(anyhow!(
                "descriptor array index {} out of bounds for binding {} with count {}",
                offset.array_index,
                binding.binding,
                count
            ));
        }

        let vk_descriptor = descriptor
            .as_any()
            .downcast_ref::<VkResourceDescriptor>()
            .ok_or_else(|| anyhow!("unsupported resource descriptor implementation"))?;

        let descriptor_type = vk_descriptor.value.descriptor_type();
        if descriptor_type != binding.descriptor_type {
            return Err(anyhow!(
                "descriptor type mismatch for binding {}: expected {:?}, got {:?}",
                binding.binding,
                binding.descriptor_type,
                descriptor_type
            ));
        }
        if binding.binding < 0 {
            return Err(anyhow!(
                "reflected binding index {} is negative for descriptor set {:?}",
                binding.binding,
                self.descriptor_layout.set
            ));
        }
        let binding_index = binding.binding as u32;

        match vk_descriptor.value {
            VkDescriptorValue::UniformBuffer(info) | VkDescriptorValue::StorageBuffer(info) => {
                let infos = [info];
                let writes = [vk::WriteDescriptorSet::builder()
                    .dst_set(self.descriptor_set)
                    .dst_binding(binding_index)
                    .dst_array_element(offset.array_index as u32)
                    .descriptor_type(descriptor_type)
                    .buffer_info(&infos)
                    .build()];
                let copies: [vk::CopyDescriptorSet; 0] = [];
                unsafe {
                    self.gpu_device
                        .get_vk_device()
                        .update_descriptor_sets(&writes, &copies);
                }
            }
            VkDescriptorValue::Sampler(info) | VkDescriptorValue::SampledImage(info) => {
                let infos = [info];
                let writes = [vk::WriteDescriptorSet::builder()
                    .dst_set(self.descriptor_set)
                    .dst_binding(binding_index)
                    .dst_array_element(offset.array_index as u32)
                    .descriptor_type(descriptor_type)
                    .image_info(&infos)
                    .build()];
                let copies: [vk::CopyDescriptorSet; 0] = [];
                unsafe {
                    self.gpu_device
                        .get_vk_device()
                        .update_descriptor_sets(&writes, &copies);
                }
            }
        }

        Ok(())
    }
}
