use std::any::Any;

use slang::{DescriptorClass, DescriptorProfile, ResourceDescriptor};
use vulkanalia::vk;

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
