use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shader_slang as slang;

use crate::reflection::ir::*;
use crate::reflection::{
    denormalize_layout_ir, ScopeIrDenorm, SubObjectRangeIrDenorm, TypeLayoutIrDenorm,
};
use crate::reflection::serde_slang::serde_binding_type;
use crate::reflection::SlangUnit;

#[derive(Default)]
struct PipelineLayoutBuilder {
    descriptor_set_layouts: Vec<Option<DescriptorSetLayoutBuilder>>,
    push_constant_ranges: Vec<PushConstantRange>,
}

impl PipelineLayoutBuilder {
    fn finish_building(&self) -> Vec<DescriptorSetLayoutBuilder> {
        let filtered: Vec<DescriptorSetLayoutBuilder> = self
            .descriptor_set_layouts
            .iter()
            .filter_map(|layout| layout.clone())
            .collect();

        filtered

        // filterOutEmptyDescriptorSets(&mut self.descriptor_set_layouts);

        // VkPipelineLayoutCreateInfo pipelineLayoutInfo = {VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO};

        // pipelineLayoutInfo.setLayoutCount = builder.descriptorSetLayouts.size();
        // pipelineLayoutInfo.pSetLayouts = builder.descriptorSetLayouts.data();

        // pipelineLayoutInfo.pushConstantRangeCount = builder.pushConstantRanges.size();
        // pipelineLayoutInfo.pPushConstantRanges = builder.pushConstantRanges.data();

        // VkPipelineLayout pipelineLayout = VK_NULL_HANDLE;
        // vkAPI.vkCreatePipelineLayout(vkAPI.device, &pipelineLayoutInfo, nullptr, &pipelineLayout);

        // *outPipelineLayout = pipelineLayout;
        // return SLANG_OK;
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DescriptorSetLayoutBuilder {
    pub set_index: u32, // XXX
    pub descriptor_ranges: Vec<DescriptorRange>,
}

impl DescriptorSetLayoutBuilder {
    fn start_building(pipeline: &mut PipelineLayoutBuilder) -> Self {
        let descriptor_set = Self {
            set_index: pipeline.descriptor_set_layouts.len() as u32,
            descriptor_ranges: vec![],
        };
        pipeline.descriptor_set_layouts.push(None);
        descriptor_set
    }

    fn finish_building(&self, pipeline: &mut PipelineLayoutBuilder) {
        if self.descriptor_ranges.is_empty() {
            return;
        }

        let i = self.set_index as usize;
        pipeline.descriptor_set_layouts[i] = Some(self.clone());
    }
}

#[derive(Clone)]
struct LayoutBuilder {
    pipeline: Arc<RwLock<PipelineLayoutBuilder>>,
    descriptor_set: Arc<RwLock<DescriptorSetLayoutBuilder>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DescriptorRange {
    pub binding_index: u32,
    pub descriptor_count: i64,
    #[serde(with = "serde_binding_type")]
    pub binding_type: slang::BindingType,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PushConstantRange {
    pub offset: u32,
    pub size: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SlangLayoutDirect {
    pub descriptor_set_layouts: Vec<DescriptorSetLayoutBuilder>,
    pub push_constant_ranges: Vec<PushConstantRange>,
}

pub fn build_layout_ir(program: &slang::ComponentType) -> Result<LayoutIr> {
    let program_layout = program.layout(0)?;
    build_layout_ir_from_program_layout(program_layout)
}

pub fn walk_program(program: &slang::ComponentType) -> Result<LayoutIr> {
    let layout_ir = build_layout_ir(program)?;
    let pipeline_layout = lower_layout_ir_to_pipeline(&layout_ir);

    for (i, descriptor_set_layout) in pipeline_layout.descriptor_set_layouts.iter().enumerate() {
        for descriptor_range in &descriptor_set_layout.descriptor_ranges {
            println!(
                "### descriptor set: set={}, binding={}, count={}, type={:?}",
                i,
                descriptor_range.binding_index,
                descriptor_range.descriptor_count,
                descriptor_range.binding_type
            );
        }
    }

    for push_constant_range in &pipeline_layout.push_constant_ranges {
        println!(
            "### push constant: offset={}, size={}",
            push_constant_range.offset, push_constant_range.size
        );
    }

    Ok(layout_ir)
}

pub fn lower_layout_ir_to_pipeline(layout_ir: &LayoutIr) -> SlangLayoutDirect {
    let denorm_layout =
        denormalize_layout_ir(layout_ir).expect("failed to denormalize layout ir");
    let mut pipeline = PipelineLayoutBuilder::default();
    let descriptor_set = DescriptorSetLayoutBuilder::start_building(&mut pipeline);
    let builder = LayoutBuilder {
        pipeline: Arc::new(RwLock::new(pipeline)),
        descriptor_set: Arc::new(RwLock::new(descriptor_set)),
    };

    add_scope_parameters(builder.clone(), &denorm_layout.global_scope);
    for entry_point in &denorm_layout.entry_points {
        add_scope_parameters(builder.clone(), &entry_point.parameters);
    }

    builder
        .descriptor_set
        .write()
        .unwrap()
        .finish_building(&mut builder.pipeline.write().unwrap());

    let descriptor_set_layouts = builder.pipeline.write().unwrap().finish_building();

    SlangLayoutDirect {
        descriptor_set_layouts,
        push_constant_ranges: builder
            .pipeline
            .write()
            .unwrap()
            .push_constant_ranges
            .clone(),
    }
}

fn add_scope_parameters(builder: LayoutBuilder, scope: &ScopeIrDenorm) {
    add_ranges_for_parameter_block_element(builder, &scope.var_layout.type_layout);
}

fn add_ranges_for_parameter_block_element(builder: LayoutBuilder, type_layout: &TypeLayoutIrDenorm) {
    if type_layout.size.bytes > 0 {
        // NOTE: for entrypoint uniform ParameterBlocks slang
        add_automatically_introduced_uniform_buffer(builder.clone());
    }

    add_ranges(builder, type_layout);
}

fn build_layout_ir_from_program_layout(
    program_layout: &slang::reflection::Shader,
) -> Result<LayoutIr> {
    let mut builder = LayoutIrBuilder::default();
    let global_var_layout = program_layout
        .global_params_var_layout()
        .context("missing global params var layout")?;
    let global_scope = ScopeIr {
        var_layout: builder.intern_var_layout(global_var_layout, 0)?,
    };

    let mut entry_points = Vec::with_capacity(program_layout.entry_point_count() as usize);
    for i in 0..program_layout.entry_point_count() {
        let entry_point = program_layout
            .entry_point_by_index(i)
            .context("missing entry point layout")?;
        let var_layout = entry_point
            .var_layout()
            .context("missing entry point var layout")?;
        entry_points.push(EntryPointIr {
            name: entry_point.name().unwrap_or_default().to_string(),
            stage: SlangEnumValue::from(entry_point.stage()),
            parameters: ScopeIr {
                var_layout: builder.intern_var_layout(var_layout, 0)?,
            },
        });
    }

    let (type_decls, types, vars) = builder.finish()?;
    let mut layout = LayoutIr {
        global_scope,
        entry_points,
        type_decls,
        types,
        vars,
    };
    canonicalize_type_layouts(&mut layout)?;
    Ok(layout)
}

fn canonicalize_type_layouts(layout: &mut LayoutIr) -> Result<()> {
    if layout.types.is_empty() {
        return Ok(());
    }

    let mut mapping: Vec<TypeLayoutId> = (0..layout.types.len())
        .map(|index| TypeLayoutId(index as u32))
        .collect();

    for _ in 0..layout.types.len() {
        let (new_mapping, unique_count) = build_type_layout_mapping(&layout.types, &layout.vars, &mapping);
        if new_mapping == mapping {
            break;
        }
        mapping = new_mapping;
        if unique_count == layout.types.len() {
            break;
        }
    }

    let mut new_types = Vec::with_capacity(mapping.iter().map(|id| id.0 as usize).max().unwrap_or(0) + 1);
    for (old_index, ty) in layout.types.iter().enumerate() {
        let new_index = mapping[old_index].0 as usize;
        if new_index == new_types.len() {
            new_types.push(ty.clone());
        }
    }

    remap_type_layout_ids(&mut new_types, &mapping);
    for var in &mut layout.vars {
        var.type_layout = mapping[var.type_layout.0 as usize];
    }

    layout.types = new_types;
    Ok(())
}

fn build_type_layout_mapping(
    types: &[TypeLayoutIr],
    vars: &[VarLayoutIr],
    mapping: &[TypeLayoutId],
) -> (Vec<TypeLayoutId>, usize) {
    let mut key_to_id: HashMap<TypeLayoutKey, TypeLayoutId> = HashMap::new();
    let mut next_id: u32 = 0;
    let mut new_mapping = vec![TypeLayoutId(0); types.len()];

    for (old_index, ty) in types.iter().enumerate() {
        let key = TypeLayoutKey::new(ty, vars, mapping);
        let id = *key_to_id.entry(key).or_insert_with(|| {
            let id = TypeLayoutId(next_id);
            next_id += 1;
            id
        });
        new_mapping[old_index] = id;
    }

    (new_mapping, next_id as usize)
}

fn remap_type_layout_ids(types: &mut [TypeLayoutIr], mapping: &[TypeLayoutId]) {
    for ty in types {
        if let Some(element) = ty.element {
            ty.element = Some(mapping[element.0 as usize]);
        }
        for sub_object in &mut ty.sub_object_ranges {
            if let Some(leaf) = sub_object.leaf_element_type_layout {
                sub_object.leaf_element_type_layout = Some(mapping[leaf.0 as usize]);
            }
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct TypeLayoutKey {
    decl: TypeDeclId,
    categories: Vec<CategoryLayoutKey>,
    size: SlangUnit,
    alignment_bytes: u32,
    stride: SlangUnit,
    stride_bytes: u32,
    matrix_layout_mode: Option<i32>,
    binding_ranges: Vec<BindingRangeKey>,
    descriptor_sets: Vec<DescriptorSetKey>,
    sub_object_ranges: Vec<SubObjectRangeKey>,
    fields: Vec<VarLayoutKey>,
    element: Option<u32>,
    element_count: Option<u32>,
    container: Option<VarLayoutKey>,
    contained: Option<VarLayoutKey>,
}

impl TypeLayoutKey {
    fn new(
        ty: &TypeLayoutIr,
        vars: &[VarLayoutIr],
        mapping: &[TypeLayoutId],
    ) -> Self {
        let fields = ty
            .fields
            .iter()
            .map(|field| VarLayoutKey::new(field.var, vars, mapping))
            .collect();
        let container = ty
            .container
            .map(|var_id| VarLayoutKey::new(var_id, vars, mapping));
        let contained = ty
            .contained
            .map(|var_id| VarLayoutKey::new(var_id, vars, mapping));
        Self {
            decl: ty.decl,
            categories: ty.categories.iter().map(CategoryLayoutKey::from).collect(),
            size: ty.size,
            alignment_bytes: ty.alignment_bytes,
            stride: ty.stride,
            stride_bytes: ty.stride_bytes,
            matrix_layout_mode: ty.matrix_layout_mode.as_ref().map(|mode| mode.value),
            binding_ranges: ty.binding_ranges.iter().map(BindingRangeKey::from).collect(),
            descriptor_sets: ty.descriptor_sets.iter().map(DescriptorSetKey::from).collect(),
            sub_object_ranges: ty
                .sub_object_ranges
                .iter()
                .map(|range| SubObjectRangeKey::new(range, mapping))
                .collect(),
            fields,
            element: ty.element.map(|id| mapping[id.0 as usize].0),
            element_count: ty.element_count,
            container,
            contained,
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct VarLayoutKey {
    offsets: Vec<CategoryOffsetKey>,
    byte_offset_delta: u32,
    binding_range_offset_delta: u32,
    type_layout: u32,
}

impl VarLayoutKey {
    fn new(var_id: VarLayoutId, vars: &[VarLayoutIr], mapping: &[TypeLayoutId]) -> Self {
        let var = &vars[var_id.0 as usize];
        Self {
            offsets: var.offsets.iter().map(CategoryOffsetKey::from).collect(),
            byte_offset_delta: var.byte_offset_delta,
            binding_range_offset_delta: var.binding_range_offset_delta,
            type_layout: mapping[var.type_layout.0 as usize].0,
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct CategoryLayoutKey {
    category: i32,
    size: u32,
    alignment: u32,
    stride: u32,
}

impl From<&CategoryLayoutIr> for CategoryLayoutKey {
    fn from(category: &CategoryLayoutIr) -> Self {
        Self {
            category: category.category.value,
            size: category.size,
            alignment: category.alignment,
            stride: category.stride,
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct CategoryOffsetKey {
    category: i32,
    offset: u32,
    space: u32,
}

impl From<&CategoryOffsetIr> for CategoryOffsetKey {
    fn from(offset: &CategoryOffsetIr) -> Self {
        Self {
            category: offset.category.value,
            offset: offset.offset,
            space: offset.space,
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct BindingRangeKey {
    binding_range_index: u32,
    binding_type: i32,
    count: u32,
    first_descriptor_range_index: u32,
}

impl From<&BindingRangeIr> for BindingRangeKey {
    fn from(range: &BindingRangeIr) -> Self {
        Self {
            binding_range_index: range.binding_range_index,
            binding_type: range.binding_type as i32,
            count: range.count,
            first_descriptor_range_index: range.first_descriptor_range_index,
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct DescriptorSetKey {
    set_index: u32,
    space_offset: u32,
    ranges: Vec<DescriptorSetRangeKey>,
}

impl From<&DescriptorSetIr> for DescriptorSetKey {
    fn from(set: &DescriptorSetIr) -> Self {
        Self {
            set_index: set.set_index,
            space_offset: set.space_offset,
            ranges: set.ranges.iter().map(DescriptorSetRangeKey::from).collect(),
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct DescriptorSetRangeKey {
    binding_type: i32,
    descriptor_count: i64,
    category: i32,
}

impl From<&DescriptorSetRangeIr> for DescriptorSetRangeKey {
    fn from(range: &DescriptorSetRangeIr) -> Self {
        Self {
            binding_type: range.binding_type as i32,
            descriptor_count: range.descriptor_count,
            category: range.category.value,
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct SubObjectRangeKey {
    binding_range_index: u32,
    binding_type: i32,
    space_offset: u32,
    leaf_element_type_layout: Option<u32>,
}

impl SubObjectRangeKey {
    fn new(range: &SubObjectRangeIr, mapping: &[TypeLayoutId]) -> Self {
        Self {
            binding_range_index: range.binding_range_index,
            binding_type: range.binding_type as i32,
            space_offset: range.space_offset,
            leaf_element_type_layout: range
                .leaf_element_type_layout
                .map(|id| mapping[id.0 as usize].0),
        }
    }
}

#[derive(Default)]
struct LayoutIrBuilder {
    type_decls: Vec<Option<TypeDeclIr>>,
    types: Vec<Option<TypeLayoutIr>>,
    vars: Vec<Option<VarLayoutIr>>,
    type_decl_ids: HashMap<usize, TypeDeclId>,
    type_ids: HashMap<usize, TypeLayoutId>,
    var_ids: HashMap<(usize, u32), VarLayoutId>,
}

impl LayoutIrBuilder {
    fn finish(self) -> Result<(Vec<TypeDeclIr>, Vec<TypeLayoutIr>, Vec<VarLayoutIr>)> {
        let type_decls = self
            .type_decls
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value.with_context(|| format!("missing type decl {index}"))
            })
            .collect::<Result<Vec<_>>>()?;
        let types = self
            .types
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value.with_context(|| format!("missing type layout {index}"))
            })
            .collect::<Result<Vec<_>>>()?;
        let vars = self
            .vars
            .into_iter()
            .enumerate()
            .map(|(index, value)| value.with_context(|| format!("missing var layout {index}")))
            .collect::<Result<Vec<_>>>()?;
        Ok((type_decls, types, vars))
    }

    fn intern_var_layout(
        &mut self,
        var_layout: &slang::reflection::VariableLayout,
        binding_range_offset_delta: u32,
    ) -> Result<VarLayoutId> {
        let key = (var_layout as *const _ as usize, binding_range_offset_delta);
        if let Some(id) = self.var_ids.get(&key) {
            return Ok(*id);
        }

        let id = VarLayoutId(self.vars.len() as u32);
        self.var_ids.insert(key, id);
        self.vars.push(None);

        let layout = self.build_var_layout(var_layout, binding_range_offset_delta)?;
        self.vars[id.0 as usize] = Some(layout);
        Ok(id)
    }

    fn build_var_layout(
        &mut self,
        var_layout: &slang::reflection::VariableLayout,
        binding_range_offset_delta: u32,
    ) -> Result<VarLayoutIr> {
        let type_layout = var_layout
            .type_layout()
            .context("missing variable type layout")?;
        let offsets = var_layout
            .categories()
            .map(|category| CategoryOffsetIr {
                category: SlangEnumValue::from(category),
                offset: var_layout.offset(category) as u32,
                space: var_layout.binding_space_with_category(category) as u32,
            })
            .collect();
        let byte_offset_delta = var_layout.offset(slang::ParameterCategory::Uniform) as u32;
        Ok(VarLayoutIr {
            name: var_layout.name().map(|name| name.to_string()),
            offsets,
            byte_offset_delta,
            binding_range_offset_delta,
            type_layout: self.intern_type_layout(type_layout)?,
        })
    }

    fn intern_type_layout(
        &mut self,
        type_layout: &slang::reflection::TypeLayout,
    ) -> Result<TypeLayoutId> {
        let key = type_layout as *const _ as usize;
        if let Some(id) = self.type_ids.get(&key) {
            return Ok(*id);
        }

        let id = TypeLayoutId(self.types.len() as u32);
        self.type_ids.insert(key, id);
        self.types.push(None);

        let layout = self.build_type_layout(type_layout)?;
        self.types[id.0 as usize] = Some(layout);
        Ok(id)
    }

    fn build_type_layout(
        &mut self,
        type_layout: &slang::reflection::TypeLayout,
    ) -> Result<TypeLayoutIr> {
        let kind = type_layout.kind();
        let categories = type_layout
            .categories()
            .map(|category| CategoryLayoutIr {
                category: SlangEnumValue::from(category),
                size: type_layout.size(category) as u32,
                alignment: type_layout.alignment(category).max(0) as u32,
                stride: type_layout.stride(category) as u32,
            })
            .collect();

        let binding_ranges = (0..type_layout.binding_range_count())
            .map(|binding_range_index| {
                let binding_type = type_layout.binding_range_type(binding_range_index);
                Ok(BindingRangeIr {
                    binding_range_index: binding_range_index as u32,
                    binding_type,
                    count: type_layout.binding_range_binding_count(binding_range_index) as u32,
                    first_descriptor_range_index: type_layout
                        .binding_range_first_descriptor_range_index(binding_range_index)
                        as u32,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let descriptor_sets = (0..type_layout.descriptor_set_count())
            .map(|set_index| {
                let ranges = (0..type_layout.descriptor_set_descriptor_range_count(set_index))
                    .map(|range_index| DescriptorSetRangeIr {
                        binding_type: type_layout
                            .descriptor_set_descriptor_range_type(set_index, range_index),
                        descriptor_count: type_layout
                            .descriptor_set_descriptor_range_descriptor_count(
                                set_index,
                                range_index,
                            ),
                        category: SlangEnumValue::from(
                            type_layout.descriptor_set_descriptor_range_category(
                                set_index,
                                range_index,
                            ),
                        ),
                    })
                    .collect();
                DescriptorSetIr {
                    set_index: set_index as u32,
                    space_offset: type_layout.descriptor_set_space_offset(set_index) as u32,
                    ranges,
                }
            })
            .collect();

        let sub_object_ranges = (0..type_layout.sub_object_range_count())
            .map(|sub_object_range_index| {
                let binding_range_index =
                    type_layout.sub_object_range_binding_range_index(sub_object_range_index);
                let leaf_element_type_layout = type_layout
                    .binding_range_leaf_type_layout(binding_range_index)
                    .and_then(|layout| layout.element_type_layout())
                    .map(|layout| self.intern_type_layout(layout))
                    .transpose()?;
                Ok(SubObjectRangeIr {
                    binding_range_index: binding_range_index as u32,
                    binding_type: type_layout.binding_range_type(binding_range_index),
                    space_offset: type_layout.sub_object_range_space_offset(
                        sub_object_range_index,
                    ) as u32,
                    leaf_element_type_layout,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let fields = if kind == slang::TypeKind::Struct {
            (0..type_layout.field_count())
                .map(|field_index| {
                    let field_layout = type_layout
                        .field_by_index(field_index)
                        .context("missing field layout")?;
                    let binding_range_offset_delta =
                        type_layout.field_binding_range_offset(field_index as i64);
                    Ok(FieldIr {
                        var: self.intern_var_layout(
                            field_layout,
                            binding_range_offset_delta as u32,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };

        let element = type_layout
            .element_type_layout()
            .map(|layout| self.intern_type_layout(layout))
            .transpose()?;

        let element_count = type_layout
            .element_count()
            .and_then(|count| (count != usize::MAX).then_some(count as u32));

        let container = type_layout
            .container_var_layout()
            .map(|layout| self.intern_var_layout(layout, 0))
            .transpose()?;

        let contained = type_layout
            .element_var_layout()
            .map(|layout| self.intern_var_layout(layout, 0))
            .transpose()?;

        let matrix_layout_mode = if kind == slang::TypeKind::Matrix {
            Some(SlangEnumValue::from(type_layout.matrix_layout_mode()))
        } else {
            None
        };

        Ok(TypeLayoutIr {
            decl: self.intern_type_decl(type_layout)?,
            categories,
            size: slang_unit_from_type_layout(type_layout),
            alignment_bytes: type_layout
                .alignment(slang::ParameterCategory::Uniform)
                .max(0) as u32,
            stride: slang_unit_from_stride(type_layout),
            stride_bytes: type_layout
                .stride(slang::ParameterCategory::Uniform) as u32,
            matrix_layout_mode,
            binding_ranges,
            descriptor_sets,
            sub_object_ranges,
            fields,
            element,
            element_count,
            container,
            contained,
        })
    }

    fn intern_type_decl(
        &mut self,
        type_layout: &slang::reflection::TypeLayout,
    ) -> Result<TypeDeclId> {
        let key = type_layout
            .ty()
            .map(|ty| ty as *const _ as usize)
            .unwrap_or(type_layout as *const _ as usize);
        if let Some(id) = self.type_decl_ids.get(&key) {
            return Ok(*id);
        }

        let id = TypeDeclId(self.type_decls.len() as u32);
        self.type_decl_ids.insert(key, id);
        self.type_decls.push(None);

        let ty = type_layout.ty();
        let name = ty
            .and_then(|ty| ty.name())
            .or_else(|| type_layout.name())
            .map(|name| name.to_string());
        let kind = ty.map(|ty| ty.kind()).unwrap_or(type_layout.kind());

        self.type_decls[id.0 as usize] = Some(TypeDeclIr {
            name,
            kind: SlangEnumValue::from(kind),
        });

        Ok(id)
    }

}

fn slang_unit_from_type_layout(type_layout: &slang::reflection::TypeLayout) -> SlangUnit {
    SlangUnit {
        set_spaces: type_layout
            .size(slang::ParameterCategory::SubElementRegisterSpace)
            as u32,
        binding_slots: type_layout
            .size(slang::ParameterCategory::DescriptorTableSlot) as u32,
        bytes: type_layout.size(slang::ParameterCategory::Uniform) as u32,
    }
}

fn slang_unit_from_stride(type_layout: &slang::reflection::TypeLayout) -> SlangUnit {
    SlangUnit {
        set_spaces: type_layout
            .stride(slang::ParameterCategory::SubElementRegisterSpace)
            as u32,
        binding_slots: type_layout
            .stride(slang::ParameterCategory::DescriptorTableSlot) as u32,
        bytes: type_layout.stride(slang::ParameterCategory::Uniform) as u32,
    }
}

fn add_automatically_introduced_uniform_buffer(builder: LayoutBuilder) {
    let binding_index = builder
        .descriptor_set
        .read()
        .unwrap()
        .descriptor_ranges
        .len() as u32;

    // VkDescriptorSetLayoutBinding binding = {};
    // binding.stageFlags = VK_SHADER_STAGE_ALL;
    // binding.binding = vulkanBindingIndex; // <-- binding_index
    // binding.descriptorCount = 1;
    // binding.descriptorType = VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER;

    let descriptor_range = DescriptorRange {
        binding_index,
        descriptor_count: 1,
        binding_type: slang::BindingType::ConstantBuffer, // need slang->vk mapping
    };

    builder
        .descriptor_set
        .write()
        .unwrap()
        .descriptor_ranges
        .push(descriptor_range);
}

fn add_ranges(builder: LayoutBuilder, type_layout: &TypeLayoutIrDenorm) {
    {
        let mut descriptor_set = builder.descriptor_set.write().unwrap();
        add_descriptor_ranges(&mut descriptor_set, type_layout);
    }
    add_sub_object_ranges(builder, type_layout);
}

fn add_descriptor_ranges(
    builder: &mut DescriptorSetLayoutBuilder,
    type_layout: &TypeLayoutIrDenorm,
) {
    // NOTE: assumes that there are no explicit bindings, otherwise we would not
    // be able to assume set indices start at 0
    let relative_set_index = 0;
    let descriptor_set = type_layout
        .descriptor_sets
        .iter()
        .find(|set| set.set_index == relative_set_index);

    let Some(descriptor_set) = descriptor_set else {
        return;
    };

    for range in &descriptor_set.ranges {
        add_descriptor_range(builder, range.binding_type, range.descriptor_count);
    }
}

fn add_descriptor_range(
    builder: &mut DescriptorSetLayoutBuilder,
    binding_type: slang::BindingType,
    descriptor_count: i64,
) {
    if binding_type == slang::BindingType::PushConstant {
        // push constants do not consume a binding slot
        return;
    }

    let descriptor_range = DescriptorRange {
        binding_index: builder.descriptor_ranges.len() as u32,
        descriptor_count,
        binding_type,
    };

    // VkDescriptorSetLayoutBinding vulkanBindingRange = {};
    // vulkanBindingRange.binding = bindingIndex;
    // vulkanBindingRange.descriptorCount = descriptorCount;
    // vulkanBindingRange.stageFlags = _currentStageFlags;
    // vulkanBindingRange.descriptorType = mapSlangBindingTypeToVulkanDescriptorType(bindingType);

    builder.descriptor_ranges.push(descriptor_range);
}

fn add_sub_object_ranges(builder: LayoutBuilder, type_layout: &TypeLayoutIrDenorm) {
    for sub_object_range in &type_layout.sub_object_ranges {
        add_sub_object_range(builder.clone(), type_layout, sub_object_range);
    }
}

fn add_sub_object_range(
    builder: LayoutBuilder,
    type_layout: &TypeLayoutIrDenorm,
    sub_object_range: &SubObjectRangeIrDenorm,
) {
    let binding_range = type_layout
        .binding_ranges
        .iter()
        .find(|range| range.binding_range_index == sub_object_range.binding_range_index);

    let Some(binding_range) = binding_range else {
        return;
    };

    match binding_range.binding_type {
        shader_slang::BindingType::ParameterBlock => {
            if let Some(element_type_layout) = sub_object_range.leaf_element_type_layout.as_deref()
            {
                add_descriptor_set_for_parameter_block(builder.pipeline.clone(), element_type_layout);
            }
        }
        shader_slang::BindingType::PushConstant => {
            if let Some(element_type_layout) = sub_object_range.leaf_element_type_layout.as_deref()
            {
                add_push_constant_range_for_constant_buffer(builder.clone(), element_type_layout);
                add_ranges(builder, element_type_layout);
            }
        }
        shader_slang::BindingType::ConstantBuffer => {
            if let Some(element_type_layout) = sub_object_range.leaf_element_type_layout.as_deref()
            {
                add_ranges_for_parameter_block_element(builder, element_type_layout);
            }
        }
        _ => {
            //
        }
    }
}

fn add_descriptor_set_for_parameter_block(
    pipeline: Arc<RwLock<PipelineLayoutBuilder>>,
    element_type_layout: &TypeLayoutIrDenorm,
) {
    let descriptor_set = DescriptorSetLayoutBuilder::start_building(&mut pipeline.write().unwrap());

    let builder = LayoutBuilder {
        pipeline,
        descriptor_set: Arc::new(RwLock::new(descriptor_set)),
    };

    add_ranges_for_parameter_block_element(builder.clone(), element_type_layout);

    builder
        .descriptor_set
        .write()
        .unwrap()
        .finish_building(&mut builder.pipeline.write().unwrap());
}

fn add_push_constant_range_for_constant_buffer(
    builder: LayoutBuilder,
    element_type_layout: &TypeLayoutIrDenorm,
) {
    let element_size = element_type_layout.size.bytes as usize;

    if element_size == 0 {
        return;
    }

    // VkPushConstantRange pushConstantRange = {};
    // pushConstantRange.stageFlags = _currentStageFlags;
    // pushConstantRange.offset = 0;
    // pushConstantRange.size = elementSize;

    let push_constant_range = PushConstantRange {
        offset: 0,
        size: element_size,
    };

    builder
        .pipeline
        .write()
        .unwrap()
        .push_constant_ranges
        .push(push_constant_range);

    // TODO: builder.pipeline.push_constants.push(...)
}
