use serde::{Deserialize, Serialize};
use shader_slang as slang;
use vulkanalia::vk;

use crate::reflection::serde_slang::serde_binding_type;
use crate::reflection::serde_vk::serde_descriptor_type;
use crate::{BindlessPolicy, SlangResourceAccess};

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
    #[serde(with = "serde_binding_type")]
    pub slang: slang::BindingType,
    #[serde(with = "serde_descriptor_type")]
    pub vk: vk::DescriptorType,
    pub access: Option<SlangResourceAccess>,
    pub binding: i64,
}

pub const BINDLESS_MUTABLE_TABLE: &[BindlessDescriptor] = &[
    BindlessDescriptor {
        slang: slang::BindingType::Sampler,
        vk: vk::DescriptorType::SAMPLER,
        access: None,
        binding: 0,
    },
    BindlessDescriptor {
        slang: slang::BindingType::CombinedTextureSampler,
        vk: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        access: None,
        binding: 1,
    },
    BindlessDescriptor {
        slang: slang::BindingType::Texture,
        vk: vk::DescriptorType::SAMPLED_IMAGE,
        access: Some(SlangResourceAccess(slang::ResourceAccess::Read)),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableTeture,
        vk: vk::DescriptorType::STORAGE_IMAGE,
        access: Some(SlangResourceAccess(slang::ResourceAccess::ReadWrite)),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::TypedBuffer,
        vk: vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        access: Some(SlangResourceAccess(slang::ResourceAccess::Read)),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableTypedBuffer,
        vk: vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        access: Some(SlangResourceAccess(slang::ResourceAccess::ReadWrite)),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::RawBuffer,
        vk: vk::DescriptorType::UNIFORM_BUFFER,
        access: Some(SlangResourceAccess(slang::ResourceAccess::Read)),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableRawBuffer,
        vk: vk::DescriptorType::STORAGE_BUFFER,
        access: Some(SlangResourceAccess(slang::ResourceAccess::ReadWrite)),
        binding: 2,
    },
    // NOTE: binding 3 is for "unknown" descriptor types
];

pub const BINDLESS_INDEXABLE_TABLE: &[BindlessDescriptor] = &[
    BindlessDescriptor {
        slang: slang::BindingType::Sampler,
        vk: vk::DescriptorType::SAMPLER,
        access: None,
        binding: 0,
    },
    BindlessDescriptor {
        slang: slang::BindingType::CombinedTextureSampler,
        vk: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        access: None,
        binding: 1,
    },
    BindlessDescriptor {
        slang: slang::BindingType::Texture,
        vk: vk::DescriptorType::SAMPLED_IMAGE,
        access: Some(SlangResourceAccess(slang::ResourceAccess::Read)),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableTeture,
        vk: vk::DescriptorType::STORAGE_IMAGE,
        access: Some(SlangResourceAccess(slang::ResourceAccess::ReadWrite)),
        binding: 3,
    },
    BindlessDescriptor {
        slang: slang::BindingType::TypedBuffer,
        vk: vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        access: Some(SlangResourceAccess(slang::ResourceAccess::Read)),
        binding: 4,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableTypedBuffer,
        vk: vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        access: Some(SlangResourceAccess(slang::ResourceAccess::ReadWrite)),
        binding: 5,
    },
    BindlessDescriptor {
        slang: slang::BindingType::RawBuffer,
        vk: vk::DescriptorType::UNIFORM_BUFFER,
        access: Some(SlangResourceAccess(slang::ResourceAccess::Read)),
        binding: 6,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableRawBuffer,
        vk: vk::DescriptorType::STORAGE_BUFFER,
        access: Some(SlangResourceAccess(slang::ResourceAccess::ReadWrite)),
        binding: 7,
    },
    // NOTE: binding 8 is for "unknown" descriptor types
];
