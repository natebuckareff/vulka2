use anyhow::Result;
use slang::{
    DescriptorClass, ResourceDescriptor, ShaderObject, ShaderOffset, ShaderParameterBlock,
    ShaderResource,
};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;

use super::{GpuBuffer, GpuBufferView, VkDescriptorValue, VkResourceDescriptor};

#[derive(Clone)]
pub struct BufferObject {
    view: GpuBufferView,
    descriptor_class: DescriptorClass,
}

impl BufferObject {
    pub fn uniform(view: GpuBufferView) -> Self {
        Self {
            view,
            descriptor_class: DescriptorClass::UniformBuffer,
        }
    }

    #[allow(dead_code)]
    pub fn storage(view: GpuBufferView) -> Self {
        Self {
            view,
            descriptor_class: DescriptorClass::StorageBuffer,
        }
    }

    pub fn whole_uniform(buffer: GpuBuffer) -> Self {
        Self::uniform(buffer.whole_view())
    }

    #[allow(dead_code)]
    pub fn whole_storage(buffer: GpuBuffer) -> Self {
        Self::storage(buffer.whole_view())
    }

    pub fn gpu_buffer(&self) -> &GpuBuffer {
        self.view.gpu_buffer()
    }

    pub fn view(&self) -> &GpuBufferView {
        &self.view
    }
}

impl ShaderResource for BufferObject {
    fn descriptor(&self) -> Box<dyn ResourceDescriptor> {
        let info = vk::DescriptorBufferInfo::builder()
            .buffer(self.view.handle())
            .offset(self.view.offset())
            .range(self.view.size())
            .build();
        let value = match self.descriptor_class {
            DescriptorClass::UniformBuffer => VkDescriptorValue::UniformBuffer(info),
            DescriptorClass::StorageBuffer => VkDescriptorValue::StorageBuffer(info),
            class => panic!("unsupported buffer descriptor class: {:?}", class),
        };
        Box::new(VkResourceDescriptor { value })
    }
}

impl ShaderObject for BufferObject {
    fn as_shader_block(&mut self) -> Option<&mut dyn ShaderParameterBlock> {
        None
    }

    fn write(&mut self, offset: ShaderOffset, bytes: &[u8]) -> Result<()> {
        self.view.write_bytes(offset.bytes, bytes)
    }
}
