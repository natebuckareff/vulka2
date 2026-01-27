use serde::{Deserialize, Serialize};
use shader_slang as slang;

use super::SlangUnit;
use crate::reflection::serde_slang::serde_binding_type;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlangEnumValue {
    pub name: String,
    pub value: i32,
}

impl From<slang::TypeKind> for SlangEnumValue {
    fn from(value: slang::TypeKind) -> Self {
        Self {
            name: format!("{value:?}"),
            value: value as i32,
        }
    }
}

impl From<slang::ParameterCategory> for SlangEnumValue {
    fn from(value: slang::ParameterCategory) -> Self {
        Self {
            name: format!("{value:?}"),
            value: value as i32,
        }
    }
}

impl From<slang::MatrixLayoutMode> for SlangEnumValue {
    fn from(value: slang::MatrixLayoutMode) -> Self {
        Self {
            name: format!("{value:?}"),
            value: value as i32,
        }
    }
}

impl From<slang::Stage> for SlangEnumValue {
    fn from(value: slang::Stage) -> Self {
        Self {
            name: format!("{value:?}"),
            value: value as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TypeLayoutId(pub u32);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VarLayoutId(pub u32);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TypeDeclId(pub u32);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutIr {
    pub global_scope: ScopeIr,
    pub entry_points: Vec<EntryPointIr>,
    pub type_decls: Vec<TypeDeclIr>,
    pub types: Vec<TypeLayoutIr>,
    pub vars: Vec<VarLayoutIr>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScopeIr {
    pub var_layout: VarLayoutId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPointIr {
    pub name: String,
    pub stage: SlangEnumValue,
    pub parameters: ScopeIr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoryLayoutIr {
    pub category: SlangEnumValue,
    pub size: u32,
    pub alignment: u32,
    pub stride: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoryOffsetIr {
    pub category: SlangEnumValue,
    pub offset: u32,
    pub space: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ShaderOffset {
    pub byte_offset: usize,
    pub binding_range_index: u32,
    pub binding_range_array_index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BindingRangeIr {
    pub binding_range_index: u32,
    #[serde(with = "serde_binding_type")]
    pub binding_type: slang::BindingType,
    pub count: u32,
    pub first_descriptor_range_index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DescriptorSetRangeIr {
    #[serde(with = "serde_binding_type")]
    pub binding_type: slang::BindingType,
    pub descriptor_count: i64,
    pub category: SlangEnumValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DescriptorSetIr {
    pub set_index: u32,
    pub space_offset: u32,
    pub ranges: Vec<DescriptorSetRangeIr>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubObjectRangeIr {
    pub binding_range_index: u32,
    #[serde(with = "serde_binding_type")]
    pub binding_type: slang::BindingType,
    pub space_offset: u32,
    pub leaf_element_type_layout: Option<TypeLayoutId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeDeclIr {
    pub name: Option<String>,
    pub kind: SlangEnumValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeLayoutIr {
    pub decl: TypeDeclId,
    pub categories: Vec<CategoryLayoutIr>,
    pub size: SlangUnit,
    pub alignment_bytes: u32,
    pub stride: SlangUnit,
    pub stride_bytes: u32,
    pub matrix_layout_mode: Option<SlangEnumValue>,
    pub binding_ranges: Vec<BindingRangeIr>,
    pub descriptor_sets: Vec<DescriptorSetIr>,
    pub sub_object_ranges: Vec<SubObjectRangeIr>,
    pub fields: Vec<FieldIr>,
    pub element: Option<TypeLayoutId>,
    pub element_count: Option<u32>,
    pub container: Option<VarLayoutId>,
    pub contained: Option<VarLayoutId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VarLayoutIr {
    pub name: Option<String>,
    pub offsets: Vec<CategoryOffsetIr>,
    pub byte_offset_delta: u32,
    pub binding_range_offset_delta: u32,
    pub type_layout: TypeLayoutId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldIr {
    pub var: VarLayoutId,
}
