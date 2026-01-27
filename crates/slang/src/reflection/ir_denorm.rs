use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shader_slang as slang;

use super::{
    BindingRangeIr, CategoryLayoutIr, CategoryOffsetIr, DescriptorSetIr, EntryPointIr,
    FieldIr as FieldIrNorm, LayoutIr, ScopeIr, SlangEnumValue, TypeDeclId, TypeLayoutId,
    VarLayoutId,
};
use super::SlangUnit;
use crate::reflection::serde_slang::serde_binding_type;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutIrDenorm {
    pub global_scope: ScopeIrDenorm,
    pub entry_points: Vec<EntryPointIrDenorm>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScopeIrDenorm {
    pub var_layout: VarLayoutIrDenorm,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPointIrDenorm {
    pub name: String,
    pub stage: SlangEnumValue,
    pub parameters: ScopeIrDenorm,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubObjectRangeIrDenorm {
    pub binding_range_index: u32,
    #[serde(with = "serde_binding_type")]
    pub binding_type: slang::BindingType,
    pub space_offset: u32,
    pub leaf_element_type_layout: Option<Box<TypeLayoutIrDenorm>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeLayoutIrDenorm {
    pub name: Option<String>,
    pub kind: SlangEnumValue,
    pub categories: Vec<CategoryLayoutIr>,
    pub size: SlangUnit,
    pub alignment_bytes: u32,
    pub stride: SlangUnit,
    pub stride_bytes: u32,
    pub matrix_layout_mode: Option<SlangEnumValue>,
    pub binding_ranges: Vec<BindingRangeIr>,
    pub descriptor_sets: Vec<DescriptorSetIr>,
    pub sub_object_ranges: Vec<SubObjectRangeIrDenorm>,
    pub fields: Vec<FieldIrDenorm>,
    pub element: Option<Box<TypeLayoutIrDenorm>>,
    pub element_count: Option<u32>,
    pub container: Option<Box<VarLayoutIrDenorm>>,
    pub contained: Option<Box<VarLayoutIrDenorm>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VarLayoutIrDenorm {
    pub name: Option<String>,
    pub offsets: Vec<CategoryOffsetIr>,
    pub byte_offset_delta: u32,
    pub binding_range_offset_delta: u32,
    pub type_layout: Box<TypeLayoutIrDenorm>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldIrDenorm {
    pub name: String,
    pub var: VarLayoutIrDenorm,
}

pub fn denormalize_layout_ir(layout: &LayoutIr) -> Result<LayoutIrDenorm> {
    Denormalizer { layout }.denormalize()
}

struct Denormalizer<'a> {
    layout: &'a LayoutIr,
}

#[derive(Clone, Copy)]
enum DenormContext {
    Full,
    SkipContainerLayouts,
}

impl<'a> Denormalizer<'a> {
    fn denormalize(&self) -> Result<LayoutIrDenorm> {
        Ok(LayoutIrDenorm {
            global_scope: self.denorm_scope(&self.layout.global_scope)?,
            entry_points: self
                .layout
                .entry_points
                .iter()
                .map(|entry| self.denorm_entry_point(entry))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn denorm_scope(&self, scope: &ScopeIr) -> Result<ScopeIrDenorm> {
        Ok(ScopeIrDenorm {
            var_layout: self.denorm_var_layout(scope.var_layout, DenormContext::Full)?,
        })
    }

    fn denorm_entry_point(&self, entry: &EntryPointIr) -> Result<EntryPointIrDenorm> {
        Ok(EntryPointIrDenorm {
            name: entry.name.clone(),
            stage: entry.stage.clone(),
            parameters: self.denorm_scope(&entry.parameters)?,
        })
    }

    fn denorm_var_layout(
        &self,
        id: VarLayoutId,
        context: DenormContext,
    ) -> Result<VarLayoutIrDenorm> {
        let var_layout = self
            .layout
            .vars
            .get(id.0 as usize)
            .with_context(|| format!("missing var layout {id:?}"))?;
        Ok(VarLayoutIrDenorm {
            name: var_layout.name.clone(),
            offsets: var_layout.offsets.clone(),
            byte_offset_delta: var_layout.byte_offset_delta,
            binding_range_offset_delta: var_layout.binding_range_offset_delta,
            type_layout: Box::new(self.denorm_type_layout(
                var_layout.type_layout,
                context,
            )?),
        })
    }

    fn denorm_type_layout(
        &self,
        id: TypeLayoutId,
        context: DenormContext,
    ) -> Result<TypeLayoutIrDenorm> {
        let type_layout = self
            .layout
            .types
            .get(id.0 as usize)
            .with_context(|| format!("missing type layout {id:?}"))?;
        let type_decl = self.type_decl(type_layout.decl)?;

        let fields = type_layout
            .fields
            .iter()
            .map(|field| self.denorm_field(field))
            .collect::<Result<Vec<_>>>()?;

        let element = type_layout
            .element
            .map(|element_id| self.denorm_type_layout(element_id, DenormContext::Full))
            .transpose()?
            .map(Box::new);

        let (container, contained) = match context {
            DenormContext::Full => {
                let container = type_layout
                    .container
                    .map(|var_id| self.denorm_var_layout(var_id, DenormContext::SkipContainerLayouts))
                    .transpose()?
                    .map(Box::new);
                let contained = type_layout
                    .contained
                    .map(|var_id| self.denorm_var_layout(var_id, DenormContext::Full))
                    .transpose()?
                    .map(Box::new);
                (container, contained)
            }
            DenormContext::SkipContainerLayouts => (None, None),
        };

        let sub_object_ranges = type_layout
            .sub_object_ranges
            .iter()
            .map(|sub_object_range| {
                let leaf_element_type_layout = sub_object_range
                    .leaf_element_type_layout
                    .map(|leaf_id| self.denorm_type_layout(leaf_id, DenormContext::Full))
                    .transpose()?
                    .map(Box::new);
                Ok(SubObjectRangeIrDenorm {
                    binding_range_index: sub_object_range.binding_range_index,
                    binding_type: sub_object_range.binding_type,
                    space_offset: sub_object_range.space_offset,
                    leaf_element_type_layout,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(TypeLayoutIrDenorm {
            name: type_decl.name.clone(),
            kind: type_decl.kind.clone(),
            categories: type_layout.categories.clone(),
            size: type_layout.size,
            alignment_bytes: type_layout.alignment_bytes,
            stride: type_layout.stride,
            stride_bytes: type_layout.stride_bytes,
            matrix_layout_mode: type_layout.matrix_layout_mode.clone(),
            binding_ranges: type_layout.binding_ranges.clone(),
            descriptor_sets: type_layout.descriptor_sets.clone(),
            sub_object_ranges,
            fields,
            element,
            element_count: type_layout.element_count,
            container,
            contained,
        })
    }

    fn denorm_field(&self, field: &FieldIrNorm) -> Result<FieldIrDenorm> {
        Ok(FieldIrDenorm {
            name: self.var_name(field.var)?,
            var: self.denorm_var_layout(field.var, DenormContext::Full)?,
        })
    }

    fn var_name(&self, var_layout: VarLayoutId) -> Result<String> {
        let var_layout = self
            .layout
            .vars
            .get(var_layout.0 as usize)
            .with_context(|| format!("missing var layout {var_layout:?}"))?;
        Ok(var_layout.name.clone().unwrap_or_default())
    }

    fn type_decl(&self, id: TypeDeclId) -> Result<&'a super::TypeDeclIr> {
        self.layout
            .type_decls
            .get(id.0 as usize)
            .with_context(|| format!("missing type decl {id:?}"))
    }

}
