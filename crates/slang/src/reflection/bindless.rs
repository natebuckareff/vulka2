use serde::{Deserialize, Serialize};
use vulkanalia::vk;

use crate::reflection::serde_vk::serde_descriptor_type;
use crate::{BindlessPolicy, SlangBindingType, SlangResourceAccess};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BindlessLayout {
    pub set: i64,
    pub policy: BindlessPolicy,
}

impl BindlessLayout {
    fn bindings(&self) -> &[BindlessDescriptor] {
        match self.policy {
            BindlessPolicy::Indexable => BINDLESS_INDEXABLE_TABLE,
            BindlessPolicy::Mutable => BINDLESS_MUTABLE_TABLE,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct BindlessDescriptor {
    pub slang: SlangBindingType,
    #[serde(with = "serde_descriptor_type")]
    pub vk: vk::DescriptorType,
    pub access: Option<SlangResourceAccess>,
    pub binding: i64,
}

pub const BINDLESS_MUTABLE_TABLE: &[BindlessDescriptor] = &[
    BindlessDescriptor {
        slang: SlangBindingType::Sampler,
        vk: vk::DescriptorType::SAMPLER,
        access: None,
        binding: 0,
    },
    BindlessDescriptor {
        slang: SlangBindingType::CombinedTextureSampler,
        vk: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        access: None,
        binding: 1,
    },
    BindlessDescriptor {
        slang: SlangBindingType::Texture,
        vk: vk::DescriptorType::SAMPLED_IMAGE,
        access: Some(SlangResourceAccess::Read),
        binding: 2,
    },
    BindlessDescriptor {
        slang: SlangBindingType::MutableTexture,
        vk: vk::DescriptorType::STORAGE_IMAGE,
        access: Some(SlangResourceAccess::ReadWrite),
        binding: 2,
    },
    BindlessDescriptor {
        slang: SlangBindingType::TypedBuffer,
        vk: vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        access: Some(SlangResourceAccess::Read),
        binding: 2,
    },
    BindlessDescriptor {
        slang: SlangBindingType::MutableTypedBuffer,
        vk: vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        access: Some(SlangResourceAccess::ReadWrite),
        binding: 2,
    },
    BindlessDescriptor {
        slang: SlangBindingType::RawBuffer,
        vk: vk::DescriptorType::UNIFORM_BUFFER,
        access: Some(SlangResourceAccess::Read),
        binding: 2,
    },
    BindlessDescriptor {
        slang: SlangBindingType::MutableRawBuffer,
        vk: vk::DescriptorType::STORAGE_BUFFER,
        access: Some(SlangResourceAccess::ReadWrite),
        binding: 2,
    },
    // NOTE: binding 3 is for "unknown" descriptor types
];

pub const BINDLESS_INDEXABLE_TABLE: &[BindlessDescriptor] = &[
    BindlessDescriptor {
        slang: SlangBindingType::Sampler,
        vk: vk::DescriptorType::SAMPLER,
        access: None,
        binding: 0,
    },
    BindlessDescriptor {
        slang: SlangBindingType::CombinedTextureSampler,
        vk: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        access: None,
        binding: 1,
    },
    BindlessDescriptor {
        slang: SlangBindingType::Texture,
        vk: vk::DescriptorType::SAMPLED_IMAGE,
        access: Some(SlangResourceAccess::Read),
        binding: 2,
    },
    BindlessDescriptor {
        slang: SlangBindingType::MutableTexture,
        vk: vk::DescriptorType::STORAGE_IMAGE,
        access: Some(SlangResourceAccess::ReadWrite),
        binding: 3,
    },
    BindlessDescriptor {
        slang: SlangBindingType::TypedBuffer,
        vk: vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        access: Some(SlangResourceAccess::Read),
        binding: 4,
    },
    BindlessDescriptor {
        slang: SlangBindingType::MutableTypedBuffer,
        vk: vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        access: Some(SlangResourceAccess::ReadWrite),
        binding: 5,
    },
    BindlessDescriptor {
        slang: SlangBindingType::RawBuffer,
        vk: vk::DescriptorType::UNIFORM_BUFFER,
        access: Some(SlangResourceAccess::Read),
        binding: 6,
    },
    BindlessDescriptor {
        slang: SlangBindingType::MutableRawBuffer,
        vk: vk::DescriptorType::STORAGE_BUFFER,
        access: Some(SlangResourceAccess::ReadWrite),
        binding: 7,
    },
    // NOTE: binding 8 is for "unknown" descriptor types
];
