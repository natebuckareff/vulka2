use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use shader_slang as slang;
use vulkanalia::vk;

use crate::SlangShaderStage;
use crate::reflection::serde_slang::serde_binding_type;

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct ShaderLayout {
    pub push_constants: Vec<PushConstantLayout>,
    pub descriptor_sets: Vec<DescriptorSet>,
    pub globals: Vec<VarLayout>,
    pub entrypoints: Vec<EntrypointLayout>,
}

// TODO: think about aliasing
#[derive(Clone, Serialize, Deserialize)]
pub struct PushConstantLayout {
    pub name: CompactString,
    pub offset_bytes: usize,
    pub size_bytes: usize,
    #[serde(with = "crate::reflection::serde_vk::serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags,
    pub element: PushConstantElementLayout,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum PushConstantElementLayout {
    Pod(PodLayout),
    Struct(StructLayout),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EntrypointLayout {
    pub stage: SlangShaderStage,
    pub params: Vec<VarLayout>,
}

// ------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct VarLayout {
    pub name: CompactString,
    pub size: Size,
    pub value: ValueLayout,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ValueLayout {
    Pod(PodLayout),
    Struct(StructLayout),
    Array(ArrayLayout),
    Resource(ResourceLayout),
    ParameterBlock(ParameterBlockLayout),
    ConstantBuffer(ConstantBufferLayout),
}

// ------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct PodLayout {
    pub offset: PodOffset,
    pub ty: PodType,
}

// NOTE: pod data does not need a "layout" since layout is a property of the
// container and pod data is always "inside" some kind of slang resource
// container
#[derive(Clone, Serialize, Deserialize)]
pub enum PodType {
    Scalar(ScalarType),
    Vector(VectorType),
    Matrix(MatrixType),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ScalarType {
    pub ty: CompactString,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VectorType {
    pub element: ScalarType,
    pub count: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MatrixType {
    pub element: ScalarType,
    pub rows: u32,
    pub cols: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StructLayout {
    pub offset: AggregateOffset,
    pub fields: Vec<VarLayout>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArrayLayout {
    pub offset: AggregateOffset,
    pub element: Box<ValueLayout>,
    pub count: ElementCount,
    pub stride: Stride,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ResourceLayout {
    pub offset: DescriptorOffset,
    pub ty: CompactString,
    #[serde(with = "serde_binding_type")]
    pub kind: ResourceKind,
    pub element: Box<ValueLayout>,
    pub count: ElementCount,
    pub stride: Stride,
}

// slang resource types
// NOTE: ParameterBlock and ConstantBuffer are excluded from this type since we
// treat them specially in the layout tree. They're not "normal" resources but
// more so "layout nodes"
// TODO: refine resource kind mapping beyond Slang binding types.
pub type ResourceKind = slang::BindingType;

// NOTE: Binding 0 is the PB's uniform buffer binding iff the PB contains any
// ordinary (uniform) data that needs wrapping. If there are no ordinary bytes,
// then there is no implicit UBO, and the first resource will be at binding 0.
#[derive(Clone, Serialize, Deserialize)]
pub struct ParameterBlockLayout {
    pub descriptor_set_index: usize, // unique for all `ParameterBlockLayout`s
    pub element: Box<ValueLayout>,
}

// resets byte offsets in the element layout
#[derive(Clone, Serialize, Deserialize)]
pub struct ConstantBufferLayout {
    pub offset: DescriptorOffset,
    pub element: Box<ValueLayout>,
}

// ------------------------------

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Size(pub usize); // bytes *excluding* padding

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Stride(pub usize); // bytes *including* padding

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ElementCount {
    Bounded(u32),
    Runtime,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum AggregateOffset {
    Pod(PodOffset),
    Descriptor(DescriptorOffset),
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PodOffset {
    pub offset_bytes: usize,
}

// NOTE: these are relative units, not addressible vulkan indices
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct DescriptorOffset {
    pub binding_index: u32, // indexes into current DescriptorSet::bindings
    pub array_index: u32,   // indexes into current descriptor array
}

// ------------------------------

// NOTE: set is None until at least one non-empty descriptor is added
#[derive(Clone, Serialize, Deserialize)]
pub struct DescriptorSet {
    pub set: Option<u32>,
    pub bindings: Vec<DescriptorBinding>,
}

// NOTE: binding is None util the descriptor is known to not be empty. Empty
// descriptors are filtered at the end of the relfection pass
#[derive(Clone, Serialize, Deserialize)]
pub struct DescriptorBinding {
    pub binding: Option<u32>,
    #[serde(with = "crate::reflection::serde_vk::serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags, // OR of all stages that use this descriptor
    pub count: ElementCount,
    pub descriptor: Descriptor,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Descriptor {
    Pod(PodDescriptor),
    Opaque(#[serde(with = "serde_binding_type")] DescriptorType), // non byte-addressible resources
}

// uniforms, ssbos, etc
#[derive(Clone, Serialize, Deserialize)]
pub struct PodDescriptor {
    pub size_bytes: usize,
    pub alignment_bytes: usize,
    #[serde(with = "serde_binding_type")]
    pub ty: DescriptorType,
    // TODO: buffer class?
}

// maps to vulkan descriptor types
// TODO: refine descriptor type mapping beyond Slang binding types.
pub type DescriptorType = slang::BindingType;
