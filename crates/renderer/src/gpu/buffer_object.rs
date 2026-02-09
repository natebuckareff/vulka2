use anyhow::{Result, anyhow};
use slang::{
    DescriptorClass, ResourceDescriptor, ShaderObject, ShaderOffset, ShaderParameterBlock,
    ShaderResource,
};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;

use super::{
    GpuBuffer, GpuBufferView, MappedBuffer, RendererWriteCtx, UploadBuffer, VkDescriptorValue,
    VkResourceDescriptor,
};

#[derive(Clone)]
enum WritePath {
    Mapped(MappedBuffer),
    Upload(UploadBuffer),
    None,
}

#[derive(Clone)]
pub struct BufferObject {
    view: GpuBufferView,
    descriptor_class: DescriptorClass,
    write_path: WritePath,
}

impl BufferObject {
    pub fn uniform_mapped(mapped: MappedBuffer) -> Self {
        Self {
            view: mapped.view(),
            descriptor_class: DescriptorClass::UniformBuffer,
            write_path: WritePath::Mapped(mapped),
        }
    }

    #[allow(dead_code)]
    pub fn storage_mapped(mapped: MappedBuffer) -> Self {
        Self {
            view: mapped.view(),
            descriptor_class: DescriptorClass::StorageBuffer,
            write_path: WritePath::Mapped(mapped),
        }
    }

    #[allow(dead_code)]
    pub fn uniform_upload(upload: UploadBuffer) -> Self {
        Self {
            view: upload.whole_view(),
            descriptor_class: DescriptorClass::UniformBuffer,
            write_path: WritePath::Upload(upload),
        }
    }

    #[allow(dead_code)]
    pub fn storage_upload(upload: UploadBuffer) -> Self {
        Self {
            view: upload.whole_view(),
            descriptor_class: DescriptorClass::StorageBuffer,
            write_path: WritePath::Upload(upload),
        }
    }

    #[allow(dead_code)]
    pub fn read_only_uniform(view: GpuBufferView) -> Self {
        Self {
            view,
            descriptor_class: DescriptorClass::UniformBuffer,
            write_path: WritePath::None,
        }
    }

    #[allow(dead_code)]
    pub fn read_only_storage(view: GpuBufferView) -> Self {
        Self {
            view,
            descriptor_class: DescriptorClass::StorageBuffer,
            write_path: WritePath::None,
        }
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

impl<'a> ShaderObject<RendererWriteCtx<'a>> for BufferObject {
    fn as_shader_block(
        &mut self,
    ) -> Option<&mut dyn ShaderParameterBlock<RendererWriteCtx<'a>>> {
        None
    }

    fn write(
        &mut self,
        ctx: &mut RendererWriteCtx<'a>,
        offset: ShaderOffset,
        bytes: &[u8],
    ) -> Result<()> {
        let local_offset = offset.bytes as vk::DeviceSize;
        match &self.write_path {
            WritePath::Mapped(mapped) => ctx
                .upload_batch
                .write_mapped(mapped, self.view.clone(), local_offset, bytes),
            WritePath::Upload(_) => ctx
                .upload_batch
                .upload_bytes(self.view.clone(), local_offset, bytes),
            WritePath::None => Err(anyhow!(
                "buffer object was created without a writable upload path"
            )),
        }
    }
}
