use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shader_slang as slang;

use crate::reflection::ir::*;
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
    let mut pipeline = PipelineLayoutBuilder::default();
    let descriptor_set = DescriptorSetLayoutBuilder::start_building(&mut pipeline);
    let builder = LayoutBuilder {
        pipeline: Arc::new(RwLock::new(pipeline)),
        descriptor_set: Arc::new(RwLock::new(descriptor_set)),
    };

    add_scope_parameters(builder.clone(), &layout_ir.global_scope);
    for entry_point in &layout_ir.entry_points {
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

fn add_scope_parameters(builder: LayoutBuilder, scope: &ScopeIr) {
    add_ranges_for_parameter_block_element(builder, &scope.var_layout.type_layout);
}

fn add_ranges_for_parameter_block_element(builder: LayoutBuilder, type_layout: &TypeLayoutIr) {
    if type_layout.size.bytes > 0 {
        // NOTE: for entrypoint uniform ParameterBlocks slang
        add_automatically_introduced_uniform_buffer(builder.clone());
    }

    add_ranges(builder, type_layout);
}

fn build_layout_ir_from_program_layout(
    program_layout: &slang::reflection::Shader,
) -> Result<LayoutIr> {
    let global_var_layout = program_layout
        .global_params_var_layout()
        .context("missing global params var layout")?;
    let global_scope = reflect_scope(global_var_layout)?;

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
            parameters: reflect_scope(var_layout)?,
        });
    }

    Ok(LayoutIr {
        global_scope,
        entry_points,
    })
}

fn reflect_scope(var_layout: &slang::reflection::VariableLayout) -> Result<ScopeIr> {
    Ok(ScopeIr {
        var_layout: reflect_var_layout(var_layout, 0, ReflectContext::Full)?,
    })
}

fn reflect_var_layout(
    var_layout: &slang::reflection::VariableLayout,
    binding_range_offset_delta: u32,
    context: ReflectContext,
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
        type_layout: Box::new(reflect_type_layout(type_layout, context)?),
    })
}

#[derive(Clone, Copy)]
enum ReflectContext {
    Full,
    SkipContainerLayouts,
}

fn reflect_type_layout(
    type_layout: &slang::reflection::TypeLayout,
    context: ReflectContext,
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
                        .descriptor_set_descriptor_range_descriptor_count(set_index, range_index),
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
                .map(|layout| reflect_type_layout(layout, ReflectContext::Full))
                .transpose()?;
            Ok(SubObjectRangeIr {
                binding_range_index: binding_range_index as u32,
                binding_type: type_layout.binding_range_type(binding_range_index),
                space_offset: type_layout.sub_object_range_space_offset(sub_object_range_index)
                    as u32,
                leaf_element_type_layout: leaf_element_type_layout.map(Box::new),
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
                    name: field_layout.name().unwrap_or_default().to_string(),
                    var: reflect_var_layout(
                        field_layout,
                        binding_range_offset_delta as u32,
                        ReflectContext::Full,
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    let element = type_layout
        .element_type_layout()
        .map(|layout| reflect_type_layout(layout, ReflectContext::Full))
        .transpose()?
        .map(Box::new);

    let element_count = type_layout
        .element_count()
        .and_then(|count| (count != usize::MAX).then_some(count as u32));

    let (container, contained) = match context {
        ReflectContext::Full => {
            let container = type_layout
                .container_var_layout()
                .map(|layout| reflect_var_layout(layout, 0, ReflectContext::SkipContainerLayouts))
                .transpose()?
                .map(Box::new);
            let contained = type_layout
                .element_var_layout()
                .map(|layout| reflect_var_layout(layout, 0, ReflectContext::Full))
                .transpose()?
                .map(Box::new);
            (container, contained)
        }
        ReflectContext::SkipContainerLayouts => (None, None),
    };

    let matrix_layout_mode = if kind == slang::TypeKind::Matrix {
        Some(SlangEnumValue::from(type_layout.matrix_layout_mode()))
    } else {
        None
    };

    Ok(TypeLayoutIr {
        name: type_layout.name().map(|name| name.to_string()),
        kind: SlangEnumValue::from(kind),
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

fn add_ranges(builder: LayoutBuilder, type_layout: &TypeLayoutIr) {
    {
        let mut descriptor_set = builder.descriptor_set.write().unwrap();
        add_descriptor_ranges(&mut descriptor_set, type_layout);
    }
    add_sub_object_ranges(builder, type_layout);
}

fn add_descriptor_ranges(builder: &mut DescriptorSetLayoutBuilder, type_layout: &TypeLayoutIr) {
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

fn add_sub_object_ranges(builder: LayoutBuilder, type_layout: &TypeLayoutIr) {
    for sub_object_range in &type_layout.sub_object_ranges {
        add_sub_object_range(builder.clone(), type_layout, sub_object_range);
    }
}

fn add_sub_object_range(
    builder: LayoutBuilder,
    type_layout: &TypeLayoutIr,
    sub_object_range: &SubObjectRangeIr,
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
    element_type_layout: &TypeLayoutIr,
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
    element_type_layout: &TypeLayoutIr,
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
