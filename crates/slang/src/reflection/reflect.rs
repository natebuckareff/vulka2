use std::sync::{Arc, RwLock};

use anyhow::Result;
use shader_slang as slang;

/// Describes the extent of a shader value. Tells us _how many_ resource units
/// the shader value consumes for its layout. Like a multi-dimensional size.
struct LayoutExtent {
    sets: u32,
    bindings: u32,
    bytes: usize,
}

// TODO
struct ShaderCursor {
    object: (), // TODO
    offset: ShaderOffset,
}

/// Describes the location of a shader value. Tells us _where_ a shader value or
/// resource is located, in terms of resource units. Like a multi-dimensional
/// pointer.
struct ShaderOffset {
    byte_offset: usize,
    binding_range_index: u32,
    array_index_in_range: u32,
}

struct VariableLayout {
    offset: ShaderOffset,
    // TODO
}

struct TypeLayout {
    size: LayoutExtent,
    stride: LayoutExtent,
    alignment_bytes: u32,
    // TODO
}

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

#[derive(Clone)]
struct DescriptorSetLayoutBuilder {
    set_index: u32,
    descriptor_ranges: Vec<DescriptorRange>,
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

#[derive(Clone)]
struct DescriptorRange {
    binding_index: u32,
    descriptor_count: i64,
    binding_type: slang::BindingType,
}

#[derive(Clone)]
struct PushConstantRange {
    offset: u32,
    size: usize,
}

pub fn walk_program(program: &slang::ComponentType) -> Result<()> {
    let program_layout = program.layout(0)?;
    create_pipeline_layout(program_layout);
    Ok(())
}

fn create_pipeline_layout(program_layout: &slang::reflection::Shader) {
    let mut pipeline = PipelineLayoutBuilder::default();
    let descriptor_set = DescriptorSetLayoutBuilder::start_building(&mut pipeline);
    let builder = LayoutBuilder {
        pipeline: Arc::new(RwLock::new(pipeline)),
        descriptor_set: Arc::new(RwLock::new(descriptor_set)),
    };

    add_global_scope_parameters(builder.clone(), program_layout);
    add_entry_point_parameters(builder.clone(), program_layout);

    builder
        .descriptor_set
        .write()
        .unwrap()
        .finish_building(&mut builder.pipeline.write().unwrap());

    let descriptor_set_layouts = builder.pipeline.write().unwrap().finish_building();

    for (i, descriptor_set_layout) in descriptor_set_layouts.iter().enumerate() {
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

    for push_constant_range in &builder.pipeline.write().unwrap().push_constant_ranges {
        println!(
            "### push constant: offset={}, size={}",
            push_constant_range.offset, push_constant_range.size
        );
    }
}

fn add_global_scope_parameters(builder: LayoutBuilder, program_layout: &slang::reflection::Shader) {
    // _currentStageFlags = VK_SHADER_STAGE_ALL
    add_ranges_for_parameter_block_element(
        builder,
        program_layout.global_params_type_layout().unwrap(),
    );
}

fn add_entry_point_parameters(builder: LayoutBuilder, program_layout: &slang::reflection::Shader) {
    let entry_point_count = program_layout.entry_point_count();

    for i in 0..entry_point_count {
        let entry_point_layout = program_layout.entry_point_by_index(i).unwrap();
        // _currentStageFlags = getShaderStageFlags(entryPointLayout->getStage());
        add_ranges_for_parameter_block_element(
            builder.clone(),
            entry_point_layout.type_layout().unwrap(),
        );
    }
}

fn add_ranges_for_parameter_block_element(
    builder: LayoutBuilder,
    type_layout: &slang::reflection::TypeLayout,
) {
    // NOTE: which category to use???
    if type_layout.size(slang::ParameterCategory::Uniform) > 0 {
        add_automatically_introduced_uniform_buffer(builder.clone());
    }

    add_ranges(builder, type_layout);
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

fn add_ranges(builder: LayoutBuilder, type_layout: &slang::reflection::TypeLayout) {
    {
        let mut descriptor_set = builder.descriptor_set.write().unwrap();
        add_descriptor_ranges(&mut descriptor_set, type_layout);
    }
    add_sub_object_ranges(builder, type_layout);
}

fn add_descriptor_ranges(
    builder: &mut DescriptorSetLayoutBuilder,
    type_layout: &slang::reflection::TypeLayout,
) {
    // NOTE: assumes that there are no explicit bindings, otherwise we would not
    // be able to assume set indices start at 0
    let relative_set_index = 0;
    let range_count = type_layout.descriptor_set_descriptor_range_count(relative_set_index);

    for range_index in 0..range_count {
        add_descriptor_range(builder, type_layout, relative_set_index, range_index);
    }
}

fn add_descriptor_range(
    builder: &mut DescriptorSetLayoutBuilder,
    type_layout: &slang::reflection::TypeLayout,
    relative_set_index: i64,
    range_index: i64,
) {
    let binding_type =
        type_layout.descriptor_set_descriptor_range_type(relative_set_index, range_index);

    let descriptor_count = type_layout
        .descriptor_set_descriptor_range_descriptor_count(relative_set_index, range_index);

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

fn add_sub_object_ranges(builder: LayoutBuilder, type_layout: &slang::reflection::TypeLayout) {
    let sub_object_range_count = type_layout.sub_object_range_count();

    for sub_object_range_index in 0..sub_object_range_count {
        add_sub_object_range(builder.clone(), type_layout, sub_object_range_index);
    }
}

fn add_sub_object_range(
    builder: LayoutBuilder,
    type_layout: &slang::reflection::TypeLayout,
    sub_object_range_index: i64,
) {
    let binding_range_index =
        type_layout.sub_object_range_binding_range_index(sub_object_range_index);

    let binding_type = type_layout.binding_range_type(binding_range_index);

    match binding_type {
        shader_slang::BindingType::ParameterBlock => {
            let parameter_block_type_layout =
                type_layout.binding_range_leaf_type_layout(binding_range_index);

            if let Some(parameter_block_type_layout) = parameter_block_type_layout {
                add_descriptor_set_for_parameter_block(
                    builder.pipeline.clone(),
                    parameter_block_type_layout,
                );
            }
        }
        shader_slang::BindingType::PushConstant => {
            let constant_buffer_type_layout =
                type_layout.binding_range_leaf_type_layout(binding_range_index);

            if let Some(type_layout) = constant_buffer_type_layout {
                add_push_constant_range_for_constant_buffer(builder.clone(), type_layout);
                add_ranges(builder, type_layout.element_type_layout().unwrap());
            }
        }
        shader_slang::BindingType::ConstantBuffer => {
            let element_type_layout = type_layout
                .binding_range_leaf_type_layout(binding_range_index)
                .map(|type_layout| type_layout.element_type_layout())
                .flatten();

            if let Some(element_type_layout) = element_type_layout {
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
    type_layout: &slang::reflection::TypeLayout,
) {
    let descriptor_set = DescriptorSetLayoutBuilder::start_building(&mut pipeline.write().unwrap());

    let builder = LayoutBuilder {
        pipeline,
        descriptor_set: Arc::new(RwLock::new(descriptor_set)),
    };

    let element_type_layout = type_layout.element_type_layout().unwrap();

    add_ranges_for_parameter_block_element(builder.clone(), element_type_layout);

    builder
        .descriptor_set
        .write()
        .unwrap()
        .finish_building(&mut builder.pipeline.write().unwrap());
}

fn add_push_constant_range_for_constant_buffer(
    builder: LayoutBuilder,
    type_layout: &slang::reflection::TypeLayout,
) {
    let element_type_layout = type_layout.element_type_layout().unwrap();
    let element_size = element_type_layout.size(slang::ParameterCategory::Uniform);

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
