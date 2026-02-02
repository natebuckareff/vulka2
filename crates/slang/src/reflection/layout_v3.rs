use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use shader_slang as slang;
use vulkanalia::vk;

use crate::SlangShaderStage;
use crate::reflection::serde_slang::serde_binding_type;
use crate::reflection::serde_slang::serde_resource_access;
use crate::reflection::serde_slang::serde_resource_shape;
use crate::reflection::serde_vk::serde_descriptor_type;
use crate::reflection::serde_vk::serde_shader_stage_flags;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct LayoutUnit {
    pub push_constants: Option<usize>,
    pub bytes: Option<usize>,
    pub bindings: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub enum ElementCount {
    Bounded(usize),
    Runtime,
}

#[derive(Serialize, Deserialize)]
pub struct ShaderLayout {
    pub globals: Option<Box<TypeLayout>>,
    pub entrypoints: Vec<EntrypointLayout>,
}

#[derive(Serialize, Deserialize)]
pub struct EntrypointLayout {
    pub name: CompactString,
    pub stage: SlangShaderStage,
    pub params: Option<Box<TypeLayout>>,
}

#[derive(Serialize, Deserialize)]
pub struct VarLayout {
    pub name: Option<CompactString>,
    pub offset_bytes: usize,
    pub offset_set: usize,
    pub offset_binding_range: i64,
    pub value: TypeLayout,
}

#[derive(Serialize, Deserialize)]
pub struct TypeLayout {
    pub size: Option<LayoutUnit>,
    pub alignment: i32,
    pub stride: usize,
    pub ty: Type,
}

#[derive(Serialize, Deserialize)]
pub enum Type {
    Unknown(String, CompactString),
    Globals(Option<Box<TypeLayout>>),
    Entrypoint(Option<Box<TypeLayout>>),
    Numeric(NumericType),
    Struct(StructType),
    Array(ArrayType),
    Resource(ResourceType),
    SamplerState(SamplerStateType),
    SamplerComparisonState(SamplerComparisonStateType),
    ParameterBlock(ParameterBlockType),
    ConstantBuffer(Box<TypeLayout>),
}

#[derive(Serialize, Deserialize)]
pub enum NumericType {
    Scalar(ScalarType),
    Vector(VectorType),
    Matrix(MatrixType),
}

#[derive(Serialize, Deserialize)]
pub struct ScalarType {
    pub ty: CompactString,
}

#[derive(Serialize, Deserialize)]
pub struct VectorType {
    pub ty: CompactString,
    pub count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct MatrixType {
    pub ty: CompactString,
    pub rows: u32,
    pub cols: u32,
}

#[derive(Serialize, Deserialize)]
pub struct StructType {
    pub name: CompactString,
    pub fields: Vec<VarLayout>,
}

#[derive(Serialize, Deserialize)]
pub struct ArrayType {
    pub count: ElementCount,
    pub element: Box<TypeLayout>,
}

#[derive(Serialize, Deserialize)]
pub struct ResourceType {
    pub ty: CompactString,
    pub binding: Option<ResourceBinding>,
    #[serde(with = "serde_resource_shape")]
    pub shape: slang::ResourceShape,
    pub access: Option<ResourceAccess>,
    pub element: Option<Box<TypeLayout>>,
}

// XXX
#[derive(Serialize, Deserialize)]
pub struct ResourceBinding(#[serde(with = "serde_binding_type")] pub slang::BindingType);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResourceShape(#[serde(with = "serde_resource_shape")] pub slang::ResourceShape);

// XXX
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResourceAccess(#[serde(with = "serde_resource_access")] pub slang::ResourceAccess);

#[derive(Serialize, Deserialize)]
pub struct SamplerStateType {
    pub is_comparison_state: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SamplerComparisonStateType {
    // TODO
}

// ~

#[derive(Serialize, Deserialize)]
pub struct ParameterBlockType {
    pub descriptor_set: DescriptorSet,
    pub element: Box<TypeLayout>,
}

#[derive(Serialize, Deserialize)]
pub struct DescriptorSet {
    pub set: i64,
    pub bindings: Vec<DescriptorBinding>,
}

#[derive(Serialize, Deserialize)]
pub struct DescriptorBinding {
    pub binding_range: Option<i64>,
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
