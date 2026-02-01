use anyhow::{Context, Result, anyhow};
use shader_slang as slang;
use vulkanalia::vk;

use crate::{BindlessConfig, BindlessPolicy, SlangShaderStage, reflection::print::PrintObject};

struct BindlessDescriptor {
    slang: slang::BindingType,
    vk: vk::DescriptorType,
    access: Option<slang::ResourceAccess>,
    binding: u32,
}

const BINDLESS_MUTABLE_TABLE: &[BindlessDescriptor] = &[
    BindlessDescriptor {
        slang: slang::BindingType::Sampler,
        vk: vk::DescriptorType::SAMPLER,
        access: None,
        binding: 0,
    },
    BindlessDescriptor {
        slang: slang::BindingType::CombinedTextureSampler,
        vk: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        access: None,
        binding: 1,
    },
    BindlessDescriptor {
        slang: slang::BindingType::Texture,
        vk: vk::DescriptorType::SAMPLED_IMAGE,
        access: Some(slang::ResourceAccess::Read),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableTeture,
        vk: vk::DescriptorType::STORAGE_IMAGE,
        access: Some(slang::ResourceAccess::ReadWrite),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::TypedBuffer,
        vk: vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        access: Some(slang::ResourceAccess::Read),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableTypedBuffer,
        vk: vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        access: Some(slang::ResourceAccess::ReadWrite),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::RawBuffer,
        vk: vk::DescriptorType::UNIFORM_BUFFER,
        access: Some(slang::ResourceAccess::Read),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableRawBuffer,
        vk: vk::DescriptorType::STORAGE_BUFFER,
        access: Some(slang::ResourceAccess::ReadWrite),
        binding: 2,
    },
    // NOTE: binding 3 is for "unknown" descriptor types
];

const BINDLESS_INDEXABLE_TABLE: &[BindlessDescriptor] = &[
    BindlessDescriptor {
        slang: slang::BindingType::Sampler,
        vk: vk::DescriptorType::SAMPLER,
        access: None,
        binding: 0,
    },
    BindlessDescriptor {
        slang: slang::BindingType::CombinedTextureSampler,
        vk: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        access: None,
        binding: 1,
    },
    BindlessDescriptor {
        slang: slang::BindingType::Texture,
        vk: vk::DescriptorType::SAMPLED_IMAGE,
        access: Some(slang::ResourceAccess::Read),
        binding: 2,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableTeture,
        vk: vk::DescriptorType::STORAGE_IMAGE,
        access: Some(slang::ResourceAccess::ReadWrite),
        binding: 3,
    },
    BindlessDescriptor {
        slang: slang::BindingType::TypedBuffer,
        vk: vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        access: Some(slang::ResourceAccess::Read),
        binding: 4,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableTypedBuffer,
        vk: vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        access: Some(slang::ResourceAccess::ReadWrite),
        binding: 5,
    },
    BindlessDescriptor {
        slang: slang::BindingType::RawBuffer,
        vk: vk::DescriptorType::UNIFORM_BUFFER,
        access: Some(slang::ResourceAccess::Read),
        binding: 6,
    },
    BindlessDescriptor {
        slang: slang::BindingType::MutableRawBuffer,
        vk: vk::DescriptorType::STORAGE_BUFFER,
        access: Some(slang::ResourceAccess::ReadWrite),
        binding: 7,
    },
    // NOTE: binding 8 is for "unknown" descriptor types
];

#[derive(Clone)]
struct State {
    print: PrintObject,
    // TODO: builder
    bindless_space_index: Option<i32>,
    base_stage: Option<SlangShaderStage>,
    base_set: i64,
}

impl State {
    fn new(print: PrintObject, bindless_space_index: Option<i32>) -> Self {
        Self {
            print,
            // TODO: builder
            bindless_space_index,
            base_stage: None,
            base_set: 0,
        }
    }

    fn set(&self, index: i64) -> i64 {
        let corrected = match self.bindless_space_index {
            Some(space_index) => {
                if index >= space_index as i64 {
                    index + 1
                } else {
                    index
                }
            }
            None => index,
        };
        self.base_set + corrected
    }

    fn set_stage(&mut self, stage: SlangShaderStage) {
        self.base_stage = Some(stage);
    }

    fn increment_set(&mut self, value: i64) {
        self.base_set += value;
    }

    fn clone_with(&self, print: PrintObject) -> Self {
        Self {
            print,
            bindless_space_index: self.bindless_space_index,
            base_stage: self.base_stage,
            base_set: self.base_set,
        }
    }
}

pub fn walk_program(
    program_layout: &slang::reflection::Shader,
    bindless_config: &Option<BindlessConfig>,
) -> Result<String> {
    let global_vars = program_layout
        .global_params_var_layout()
        .context("global_params_var_layout failed")?;

    let print = PrintObject::new(0).object("program");

    let space_index = if let Some(config) = bindless_config {
        let table = match config.policy {
            BindlessPolicy::Indexable => BINDLESS_INDEXABLE_TABLE,
            BindlessPolicy::Mutable => BINDLESS_MUTABLE_TABLE,
        };
        let print = print.array("bindless");
        for entry in table {
            let mut print_item = print.object();

            print_item.value("set", &config.space_index.to_string());
            print_item.value("binding", &entry.binding.to_string());
            print_item.value("type", &format!("{:?} / {:?}", entry.slang, entry.vk));
            print_item.value("count", "-1");
            print_item.value("access", &format!("{:?}", entry.access));
        }
        Some(config.space_index)
    } else {
        None
    };

    let print_globals = print.object("globals");
    let state = State::new(print_globals, space_index);
    print_var_layout(state.clone(), global_vars)?;

    let print_eps = print.array("entrypoints");
    let entry_point_count = program_layout.entry_point_count();
    for i in 0..entry_point_count {
        let entry_point = program_layout
            .entry_point_by_index(i)
            .context("entry_point_by_index failed")?;

        let entrypoint_vars = entry_point
            .var_layout()
            .context("entry_point.var_layout failed")?;

        let mut print_ep = print_eps.object();
        print_ep.value(
            "name",
            entry_point.name().context("entry_point.name failed")?,
        );

        let stage = entry_point.stage().try_into()?;

        let mut state = state.clone_with(print_ep);
        state.set_stage(stage);

        print_var_layout(state, entrypoint_vars)?;
    }

    Ok(print.read_buffer())
}

fn print_var_layout(
    mut state: State,
    var_layout: &slang::reflection::VariableLayout,
) -> Result<()> {
    if let Some(name) = var_layout.name() {
        state.print.value("name", name);
    }

    let categories = var_layout.categories();
    if categories.len() > 0 {
        let print_array = state.print.array("offset");
        for category in categories {
            let value = var_layout.offset(category);
            // TODO: serdes
            let unit = format!("{:?}", category);

            if category == slang::ParameterCategory::SubElementRegisterSpace {
                state.increment_set(value as i64);
            }

            let mut print_object = print_array.object();
            print_object.value("value", &value.to_string());
            print_object.value("unit", &unit);
        }
    }

    if let Some(type_layout) = var_layout.type_layout() {
        print_type_layout(state, type_layout, false)?;
    }

    Ok(())
}

fn print_type_layout(
    state: State,
    type_layout: &slang::reflection::TypeLayout,
    is_parameter_block: bool,
) -> Result<()> {
    let mut print = state.print.object("layout_type");

    let kind = type_layout.kind();
    // TODO: serdes
    print.value("kind", &format!("{:?}", kind));

    if kind == slang::TypeKind::Struct {
        if let Some(name) = type_layout.name() {
            print.value("name", name);
        }
    }

    let categories = type_layout.categories();
    if categories.len() > 0 {
        let print_array = print.array("size");
        for category in categories {
            let value = type_layout.size(category);
            // TODO: serdes
            let unit = format!("{:?}", category);
            let mut print_object = print_array.object();
            print_object.value("value", &value.to_string());
            print_object.value("unit", &unit);
        }
    }

    if let Some(element_count) = type_layout.element_count() {
        print.value("element_count", &element_count.to_string());
    }

    let alignment = type_layout.alignment(slang::ParameterCategory::Uniform);
    let stride = type_layout.stride(slang::ParameterCategory::Uniform);
    if alignment != 1 || stride != 0 {
        print.value("alignment", &alignment.to_string());
        print.value("stride", &stride.to_string());
    }

    if is_parameter_block {
        let print = print.array("descriptors");
        let binding_range_count = type_layout.binding_range_count();

        if type_layout.size(slang::ParameterCategory::Uniform) > 0 {
            let range_index = 0;
            let set_index = type_layout.binding_range_descriptor_set_index(range_index);
            let vk_set = state.set(type_layout.descriptor_set_space_offset(set_index));
            let vk_binding_type = vk::DescriptorType::UNIFORM_BUFFER;
            let vk_binding_count = 1;

            let mut print = print.object();
            print.value("set", &vk_set.to_string());
            print.value("binding", "0");
            print.value("type", &format!("{:?}", vk_binding_type));
            print.value("count", &vk_binding_count.to_string());

            // TODO: add descriptor to builder
        }

        for br in 0..binding_range_count {
            let dr_count = type_layout.binding_range_descriptor_range_count(br);
            if dr_count == 0 {
                continue;
            }

            let set_index = type_layout.binding_range_descriptor_set_index(br);
            let vk_set = state.set(type_layout.descriptor_set_space_offset(set_index));

            let first = type_layout.binding_range_first_descriptor_range_index(br);
            for i in 0..dr_count {
                let range_index = first + i;
                let vk_binding = type_layout
                    .descriptor_set_descriptor_range_index_offset(set_index, range_index);

                let binding_type =
                    type_layout.descriptor_set_descriptor_range_type(set_index, range_index);

                // let resource_access = type_layout
                //     .descriptor_set_descriptor_range_resource_access(set_index, range_index);

                let vk_binding_type = map_descriptor_type(binding_type)?;

                let vk_binding_count = type_layout
                    .descriptor_set_descriptor_range_descriptor_count(set_index, range_index);

                // if vk_binding_count < 0 -> runtime sized

                let mut print = print.object();
                print.value("set", &vk_set.to_string());
                print.value("binding", &vk_binding.to_string());
                print.value(
                    "type",
                    // TODO: serdes
                    &format!("{:?} / {:?}", binding_type, vk_binding_type),
                );
                print.value("count", &vk_binding_count.to_string());

                // TODO: add descriptor to builder
            }
        }
    }

    match type_layout.kind() {
        slang::TypeKind::Struct => {
            let fields = type_layout.fields();
            let print = print.array("fields");
            for field in fields {
                let print = print.object();
                let next_state = state.clone_with(print);
                print_var_layout(next_state, field)?;
            }
        }
        slang::TypeKind::Resource => {
            let shape = type_layout.resource_shape();
            let access = type_layout.resource_access();

            // TODO: serdes
            print.value("shape", &format!("{:?}", shape));
            print.value("access", &format!("{:?}", access));

            if let Some(element_type) = type_layout.element_type_layout() {
                let print = print.object("element layout");
                print_type_layout(state.clone_with(print), element_type, false)?;
            }
        }
        slang::TypeKind::ParameterBlock => {
            if let Some(element_type) = type_layout.element_type_layout() {
                let print = print.object("element layout");
                print_type_layout(state.clone_with(print), element_type, true)?;
            }
        }
        slang::TypeKind::ConstantBuffer => {
            if let Some(element_type) = type_layout.element_type_layout() {
                let print = print.object("element layout");
                print_type_layout(state.clone_with(print), element_type, false)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn map_descriptor_type(binding_type: slang::BindingType) -> Result<vk::DescriptorType> {
    use slang::BindingType::*;
    match binding_type {
        Sampler => Ok(vk::DescriptorType::SAMPLER),
        CombinedTextureSampler => Ok(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        Texture => Ok(vk::DescriptorType::SAMPLED_IMAGE),
        MutableTeture => Ok(vk::DescriptorType::STORAGE_IMAGE),
        TypedBuffer => Ok(vk::DescriptorType::UNIFORM_TEXEL_BUFFER),
        MutableTypedBuffer => Ok(vk::DescriptorType::STORAGE_TEXEL_BUFFER),
        RawBuffer => Ok(vk::DescriptorType::STORAGE_BUFFER),
        MutableRawBuffer => Ok(vk::DescriptorType::STORAGE_BUFFER),
        InputRenderTarget => Ok(vk::DescriptorType::INPUT_ATTACHMENT),
        InlineUniformData => Ok(vk::DescriptorType::INLINE_UNIFORM_BLOCK),
        RayTracingAccelerationStructure => Ok(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR),
        ConstantBuffer => Ok(vk::DescriptorType::UNIFORM_BUFFER),
        PushConstant => Err(anyhow!("push constant bindings are not descriptors")),
        _ => Err(anyhow!("unsupported binding type")),
    }
}
