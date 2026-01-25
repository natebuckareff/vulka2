//! Shader layout types extracted from Slang reflection.
//!
//! These types represent the binding and memory layout information needed
//! to create Vulkan pipeline layouts, descriptor set layouts, and push
//! constant ranges.

use std::ops::{Add, Mul};

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use vulkanalia::vk;

use super::types::{SlangResource, SlangStruct, SlangType};
use crate::compiler::{SlangEntrypoint, SlangShaderStage};
use crate::reflection::serde_vk::*;

/// A multi-dimensional unit for tracking layout offsets and sizes.
///
/// In the context of the Slang reflection API:
/// - `set_spaces` maps to the `SubElementRegisterSpace` category (descriptor sets)
/// - `binding_slots` maps to the `DescriptorTableSlot` category (bindings)
/// - `bytes` maps to the `Uniform` category (ordinary data)
///
/// These values are _relative_ and _accumulated_ while walking the reflection
/// API tree structure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlangUnit {
    pub set_spaces: u32,
    pub binding_slots: u32,
    pub bytes: u32,
}

impl SlangUnit {
    pub const fn zero() -> Self {
        Self {
            set_spaces: 0,
            binding_slots: 0,
            bytes: 0,
        }
    }

    pub const fn from_bytes(bytes: u32) -> Self {
        Self {
            set_spaces: 0,
            binding_slots: 0,
            bytes,
        }
    }

    pub const fn from_bindings(binding_slots: u32) -> Self {
        Self {
            set_spaces: 0,
            binding_slots,
            bytes: 0,
        }
    }

    pub const fn from_sets(set_spaces: u32) -> Self {
        Self {
            set_spaces,
            binding_slots: 0,
            bytes: 0,
        }
    }
}

impl Add for SlangUnit {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            set_spaces: self.set_spaces + rhs.set_spaces,
            binding_slots: self.binding_slots + rhs.binding_slots,
            bytes: self.bytes + rhs.bytes,
        }
    }
}

impl Mul<u32> for SlangUnit {
    type Output = Self;

    fn mul(self, rhs: u32) -> Self::Output {
        Self {
            set_spaces: self.set_spaces * rhs,
            binding_slots: self.binding_slots * rhs,
            bytes: self.bytes * rhs,
        }
    }
}

/// The root layout structure for a compiled Slang program.
///
/// Contains all the information needed to:
/// - Create Vulkan descriptor set layouts
/// - Create Vulkan pipeline layouts
/// - Build shader cursors for parameter passing
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlangLayout {
    /// Bindless heap layout, if bindless mode is enabled.
    pub bindless_heap: Option<BindlessHeapLayout>,
    /// Push constant layout, if push constants are used.
    pub push_constants: Option<PushConstantLayout>,
    /// Flattened descriptor set layouts for Vulkan pipeline creation.
    pub descriptor_sets: Vec<DescriptorSetLayout>,
    /// Hierarchical parameter block layouts for shader cursor traversal.
    pub parameter_blocks: Vec<ParameterBlockLayout>,
    /// Per-entrypoint layouts (vertex inputs, etc.).
    pub entrypoints: Vec<EntrypointLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindlessHeapLayout {
    pub set: u32,
    pub policy: BindlessPolicy,
    pub bindings: Vec<DescriptorBindingLayout>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BindlessPolicy {
    /// Standard descriptor indexing on a runtime-sized descriptor set.
    DescriptorIndexing,
    /// VK_EXT_mutable_descriptor_type based approach.
    MutableDescriptor,
    /// VK_EXT_descriptor_buffer based approach.
    DescriptorBuffer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushConstantLayout {
    #[serde(with = "serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags,
    pub size_bytes: u32,
    pub ty: SlangStruct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescriptorSetLayout {
    pub set: u32,
    pub bindings: Vec<DescriptorBindingLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescriptorBindingLayout {
    pub binding: u32,
    pub name: CompactString,
    #[serde(with = "serde_descriptor_binding_flags")]
    pub flags: vk::DescriptorBindingFlags,
    #[serde(with = "serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags,
    pub ty: SlangResource,
    pub count: DescriptorCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DescriptorCount {
    Count(u32),
    Variable,
}

impl DescriptorCount {
    pub fn map<F: FnOnce(u32) -> u32>(self, f: F) -> Self {
        match self {
            DescriptorCount::Count(n) => DescriptorCount::Count(f(n)),
            DescriptorCount::Variable => DescriptorCount::Variable,
        }
    }

    pub fn unwrap_or(self, default: u32) -> u32 {
        match self {
            DescriptorCount::Count(n) => n,
            DescriptorCount::Variable => default,
        }
    }

    pub fn get(self) -> Option<u32> {
        match self {
            DescriptorCount::Count(n) => Some(n),
            DescriptorCount::Variable => None,
        }
    }

    pub fn is_variable(self) -> bool {
        matches!(self, DescriptorCount::Variable)
    }
}

/// Layout for a parameter block (hierarchical shader object structure).
///
/// Parameter blocks preserve the structure of the shader code and are used
/// for shader cursor traversal and CPU-side parameter passing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterBlockLayout {
    /// Scope of this parameter block (global or per-entrypoint).
    pub scope: ParameterBlockScope,
    /// Name of the parameter block.
    pub name: CompactString,
    /// Type of the parameter block contents.
    pub ty: SlangType,
    /// Vulkan descriptor set index for this block.
    pub set: u32,
    /// Binding for ordinary (uniform) data, if any.
    pub ordinary: Option<OrdinaryParameterBinding>,
    /// Descriptor bindings within this block.
    pub bindings: Vec<DescriptorBindingLayout>,
    /// Nested parameter blocks.
    pub nested: Vec<ParameterBlockLayout>,
}

/// Scope of a parameter block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterBlockScope {
    /// Global scope (accessible from all entrypoints).
    Global,
    /// Scoped to a specific entrypoint.
    Entrypoint(SlangEntrypoint),
}

/// Binding information for ordinary (uniform/constant) data within a parameter block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryParameterBinding {
    /// Binding index for the constant buffer.
    pub binding: u32,
    /// Type information for the ordinary data.
    pub ty: SlangStruct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntrypointLayout {
    pub entrypoint: SlangEntrypoint,
    pub vertex_inputs: Option<VertexInputLayout>,
    pub compute_thread_group_size: Option<[u32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VertexInputLayout {
    pub attributes: Vec<VertexAttributeLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VertexAttributeLayout {
    pub location: u32,
    pub name: CompactString,
    #[serde(with = "serde_format")]
    pub format: vk::Format,
    /// Suggested binding index (reasonable default). Used by generated shader
    /// cursor.
    pub hint_binding: u32,
    /// Suggested offset within the binding (reasonable default). Used by
    /// generated shader cursor.
    pub hint_offset: u32,
}

impl From<SlangShaderStage> for vk::ShaderStageFlags {
    fn from(stage: SlangShaderStage) -> Self {
        match stage {
            SlangShaderStage::Vertex => vk::ShaderStageFlags::VERTEX,
            SlangShaderStage::Fragment => vk::ShaderStageFlags::FRAGMENT,
            SlangShaderStage::Compute => vk::ShaderStageFlags::COMPUTE,
        }
    }
}
