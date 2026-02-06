use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use shader_slang as slang;
use vulkanalia::vk;

use crate::BindlessLayout;
use crate::SlangShaderStage;
use crate::reflection::serde_slang::serde_binding_type;
use crate::reflection::serde_slang::serde_resource_access;
use crate::reflection::serde_slang::serde_resource_shape;
use crate::reflection::serde_vk::serde_descriptor_type;
use crate::reflection::serde_vk::serde_shader_stage_flags;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct LayoutUnit {
    pub push_constants: Option<usize>,
    pub bytes: Option<usize>,
    pub bindings: Option<usize>,
    pub varying_input: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ElementCount {
    Bounded(usize),
    Runtime,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ShaderLayout {
    pub bindless: Option<BindlessLayout>,
    pub globals: Option<Box<VarLayout>>,
    pub entrypoints: Vec<EntrypointLayout>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EntrypointLayout {
    pub name: CompactString,
    pub stage: SlangShaderStage,
    pub params: Option<Box<VarLayout>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VarLayout {
    pub name: Option<CompactString>,
    pub offset_bytes: usize,
    pub offset_set: usize,
    pub offset_binding_range: i64,
    pub stage: Option<StageVarLayout>,
    pub value: TypeLayout,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum StageVarLayout {
    Vertex(VertexVarLayout),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VertexVarLayout {
    pub offset_input: usize,
    pub index: usize,
    pub name: Option<CompactString>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TypeLayout {
    pub size: Option<LayoutUnit>,
    pub alignment: i32,
    pub stride: Stride,
    pub ty: Type,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Stride {
    pub bytes: usize,
    pub binding_range: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Type {
    Unknown(String, CompactString),
    Pointer(PointerType),
    Numeric(NumericType),
    Struct(StructType),
    Array(ArrayType),
    Resource(ResourceType),
    SamplerState(SamplerStateType),
    SamplerComparisonState(SamplerComparisonStateType),
    ParameterBlock(ParameterBlockType),
    ConstantBuffer(Box<TypeLayout>),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PointerType;

#[derive(Debug, Deserialize, Serialize)]
pub enum NumericType {
    Scalar(ScalarType),
    Vector(VectorType),
    Matrix(MatrixType),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScalarType {
    pub ty: CompactString,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VectorType {
    pub ty: CompactString,
    pub count: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MatrixType {
    pub ty: CompactString,
    pub rows: u32,
    pub cols: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StructType {
    pub name: CompactString,
    pub fields: Vec<VarLayout>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ArrayType {
    pub count: ElementCount,
    pub element: Box<TypeLayout>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResourceType {
    pub ty: CompactString,
    pub binding: Option<ResourceBinding>,
    #[serde(with = "serde_resource_shape")]
    pub shape: slang::ResourceShape,
    pub access: Option<ResourceAccess>,
    pub element: Option<Box<TypeLayout>>,
}

// XXX
#[derive(Debug, Deserialize, Serialize)]
pub struct ResourceBinding(#[serde(with = "serde_binding_type")] pub slang::BindingType);

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ResourceShape(#[serde(with = "serde_resource_shape")] pub slang::ResourceShape);

// XXX
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ResourceAccess(#[serde(with = "serde_resource_access")] pub slang::ResourceAccess);

#[derive(Debug, Deserialize, Serialize)]
pub struct SamplerStateType {
    pub is_comparison_state: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SamplerComparisonStateType {
    // TODO
}

// ~

#[derive(Debug, Deserialize, Serialize)]
pub struct ParameterBlockType {
    pub descriptor_set: DescriptorSet,
    pub element: Box<TypeLayout>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DescriptorSet {
    pub set: i64,
    pub implicit_ubo: Option<DescriptorBinding>,
    pub binding_ranges: Vec<BindingRange>,
}

impl DescriptorSet {
    pub fn find_binding_range(&self, range_index: i64) -> Option<&BindingRange> {
        self.binding_ranges
            .iter()
            .find(|br| br.range_index == range_index)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BindingRange {
    pub range_index: i64,
    pub descriptor: DescriptorBinding,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DescriptorBinding {
    pub binding: i64,
    #[serde(with = "serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags,
    #[serde(with = "serde_binding_type")]
    pub binding_type: slang::BindingType,
    #[serde(with = "serde_descriptor_type")]
    pub descriptor_type: vk::DescriptorType,
    pub shape: Option<ResourceShape>,
    pub access: Option<ResourceAccess>,
    pub count: ElementCount,
}
