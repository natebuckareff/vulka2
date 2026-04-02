use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use vulkanalia::vk;

use crate::BindlessLayout;
use crate::SlangBindingType;
use crate::SlangResourceAccess;
use crate::SlangResourceShape;
use crate::SlangShaderStage;
use crate::reflection::serde_vk::serde_descriptor_type;
use crate::reflection::serde_vk::serde_shader_stage_flags;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct LayoutSize {
    pub push_constants: Option<usize>,
    pub bytes: Option<usize>,
    pub bindings: Option<usize>,
    pub varying_input: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum ElementCount {
    Bounded(usize),
    Runtime,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShaderLayout {
    pub bindless: Option<BindlessLayout>,
    pub globals: Option<Box<VarLayout>>,
    pub entrypoints: Vec<EntrypointLayout>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EntrypointLayout {
    pub name: CompactString,
    pub stage: SlangShaderStage,
    pub params: Option<Box<VarLayout>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VarLayout {
    pub name: Option<CompactString>,
    pub offset_bytes: usize,
    pub offset_set: usize,
    pub offset_binding_range: i64,
    pub varying: Option<VaryingLayout>,
    pub value: TypeLayout,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VaryingLayout {
    pub offset_input: usize,
    pub index: usize,
    pub name: Option<CompactString>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TypeLayout {
    pub size: Option<LayoutSize>,
    pub alignment: i32,
    pub stride: Stride,
    pub ty: Type,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Stride {
    pub bytes: usize,
    pub binding_range: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    PushConstantBuffer(PushConstantBufferType),
    ConstantBuffer(Box<TypeLayout>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PointerType {
    pub element: Box<TypeLayout>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum NumericType {
    Scalar(ScalarType),
    Vector(VectorType),
    Matrix(MatrixType),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScalarType {
    pub ty: CompactString,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VectorType {
    pub ty: CompactString,
    pub count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MatrixType {
    pub ty: CompactString,
    pub rows: u32,
    pub cols: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StructType {
    pub name: CompactString,
    pub fields: Vec<VarLayout>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArrayType {
    pub count: ElementCount,
    pub element: Box<TypeLayout>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResourceType {
    pub ty: CompactString,
    pub binding: Option<SlangBindingType>,
    pub shape: SlangResourceShape,
    pub access: Option<SlangResourceAccess>,
    pub element: Option<Box<TypeLayout>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SamplerStateType {
    pub is_comparison_state: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SamplerComparisonStateType {
    // TODO
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParameterBlockType {
    pub layout: ParameterBlockLayout,
    pub element: Box<TypeLayout>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PushConstantBufferType {
    pub layout: PushConstantRangeLayout,
    pub element: Box<TypeLayout>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct PushConstantRangeLayout {
    #[serde(with = "serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags,
    pub offset: u32,
    pub size: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ParameterBlockLayout {
    pub set: Option<i64>,
    pub implicit_ubo: Option<DescriptorBindingLayout>,
    pub binding_ranges: Vec<BindingRangeLayout>,
}

impl ParameterBlockLayout {
    pub fn find_binding_range(&self, range_index: i64) -> Option<&BindingRangeLayout> {
        self.binding_ranges
            .iter()
            .find(|br| br.range_index == range_index)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct BindingRangeLayout {
    pub range_index: i64,
    pub descriptor: DescriptorBindingLayout,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct DescriptorBindingLayout {
    pub binding: i64,
    #[serde(with = "serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags,
    pub binding_type: SlangBindingType,
    #[serde(with = "serde_descriptor_type")]
    pub descriptor_type: vk::DescriptorType,
    pub shape: Option<SlangResourceShape>,
    pub access: Option<SlangResourceAccess>,
    pub count: ElementCount,
}
