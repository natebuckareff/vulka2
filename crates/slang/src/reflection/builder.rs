use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use compact_str::ToCompactString;
use shader_slang as slang;
use vulkanalia::vk;

use crate::{BindlessConfig, BindlessLayout, SlangShaderStage, reflection::layout::*};

#[derive(Debug)]
enum BuilderLocation {
    Global,
    Entrypoint(SlangShaderStage),
}

struct DescriptorMeta {
    shape: slang::ResourceShape,
    access: Option<ResourceAccess>,
}

pub struct LayoutBuilder {
    location: BuilderLocation,
    base_bytes: usize,
    base_set: usize,
    base_binding_range: i64,
    descriptors: HashMap<(i64, i64), DescriptorMeta>,
    bindless_config: Option<BindlessConfig>,
}

impl LayoutBuilder {
    pub fn new(bindless_config: Option<BindlessConfig>) -> Self {
        Self {
            location: BuilderLocation::Global,
            base_bytes: 0,
            base_set: 0,
            base_binding_range: 0,
            descriptors: HashMap::new(),
            bindless_config,
        }
    }

    pub fn build(&mut self, program_layout: &slang::reflection::Shader) -> Result<ShaderLayout> {
        let bindless = self.bindless_config.take().map(|config| BindlessLayout {
            set: config.space_index as i64,
            policy: config.policy,
        });

        let global_var_layout = program_layout
            .global_params_var_layout()
            .context("global var layout not found")?;

        let globals = self.build_var_layout(global_var_layout, 0)?.map(Box::new);

        let shader_layout = ShaderLayout {
            bindless,
            globals,
            entrypoints: self.build_entrypoints(program_layout)?,
        };

        Ok(shader_layout)
    }

    fn build_entrypoints(
        &mut self,
        program_layout: &slang::reflection::Shader,
    ) -> Result<Vec<EntrypointLayout>> {
        let count = program_layout.entry_point_count();
        let mut entrypoints = Vec::with_capacity(count as usize);
        for i in 0..count {
            let slang_entry_point = program_layout
                .entry_point_by_index(i)
                .context("entry_point_by_index failed")?;

            let name = slang_entry_point
                .name()
                .context("entrypoint name not found")?
                .to_compact_string();

            let stage: SlangShaderStage = slang_entry_point.stage().try_into()?;

            let slang_var_layout = slang_entry_point
                .var_layout()
                .context("entrypoint var layout not found")?;

            self.location = BuilderLocation::Entrypoint(stage);

            let params = self.build_var_layout(slang_var_layout, 0)?.map(Box::new);

            self.location = BuilderLocation::Global;

            entrypoints.push(EntrypointLayout {
                name,
                stage,
                params,
            });
        }
        Ok(entrypoints)
    }

    fn build_var_layout(
        &mut self,
        slang_var_layout: &slang::reflection::VariableLayout,
        offset_binding_range: i64,
    ) -> Result<Option<VarLayout>> {
        let name = slang_var_layout.name().map(|name| name.to_compact_string());
        let offset_bytes = slang_var_layout.offset(slang::ParameterCategory::Uniform);
        let offset_set = slang_var_layout.offset(slang::ParameterCategory::SubElementRegisterSpace);
        let varying = self.build_varying_layout(slang_var_layout);

        self.base_bytes += offset_bytes;
        self.base_set += offset_set;
        self.base_binding_range += offset_binding_range;

        let slang_type_layout = slang_var_layout
            .type_layout()
            .context("type layout not found")?;

        let Some(value) = self.build_type_layout(slang_type_layout)? else {
            return Ok(None);
        };

        self.base_bytes -= offset_bytes;
        self.base_set -= offset_set;
        self.base_binding_range -= offset_binding_range;

        let var_layout = VarLayout {
            name,
            offset_bytes,
            offset_set,
            offset_binding_range,
            varying,
            value,
        };
        Ok(Some(var_layout))
    }

    fn build_varying_layout(
        &mut self,
        slang_var_layout: &slang::reflection::VariableLayout,
    ) -> Option<VaryingLayout> {
        match slang_var_layout.stage() {
            slang::Stage::Vertex => {
                let offset_input = slang_var_layout.offset(slang::ParameterCategory::VaryingInput);
                let index = slang_var_layout.semantic_index();
                let name = slang_var_layout
                    .semantic_name()
                    .map(|name| name.to_compact_string());
                let layout = VaryingLayout {
                    offset_input,
                    index,
                    name,
                };
                Some(layout)
            }
            _ => None,
        }
    }

    fn build_type_layout(
        &mut self,
        slang_type_layout: &slang::reflection::TypeLayout,
    ) -> Result<Option<TypeLayout>> {
        let size = Self::build_size(slang_type_layout);
        let alignment = slang_type_layout.alignment(slang::ParameterCategory::Uniform);
        let stride = Stride {
            bytes: slang_type_layout.stride(slang::ParameterCategory::Uniform),
            binding_range: slang_type_layout.binding_range_count(),
        };

        let ty = match slang_type_layout.kind() {
            slang::TypeKind::Pointer => {
                // NOTE: it doesn't seem useful at this point to reflect on the
                // pointee type, but we could in the future. One challenge is
                // figuring out how to avoid duplicating the struct type in the
                // layout tree. The slang reflection API does not give us a
                // stable type-node identifier
                Type::Pointer(PointerType)
            }
            slang::TypeKind::Scalar => {
                let ty = slang_type_layout
                    .name()
                    .context("scalar type name not found")?
                    .to_compact_string();
                let scalar_type = ScalarType { ty };
                Type::Numeric(NumericType::Scalar(scalar_type))
            }
            slang::TypeKind::Vector => {
                let count = slang_type_layout
                    .element_count()
                    .context("vector element count not found")?;

                let slang_element_type = slang_type_layout
                    .element_type_layout()
                    .context("vector element type not found")?;

                let ty = slang_element_type
                    .name()
                    .context("vector element type name not found")?
                    .to_compact_string();

                let vector_type = VectorType { ty, count };
                Type::Numeric(NumericType::Vector(vector_type))
            }
            slang::TypeKind::Matrix => {
                let rows = slang_type_layout
                    .row_count()
                    .context("matrix row count not found")?;

                let cols = slang_type_layout
                    .column_count()
                    .context("matrix column count not found")?;

                let slang_element_type = slang_type_layout
                    .element_type_layout()
                    .context("matrix outer element type not found")?
                    .element_type_layout()
                    .context("matrix inner element type not found")?;

                let ty = slang_element_type
                    .name()
                    .context("matrix element type name not found")?
                    .to_compact_string();

                let matrix_type = MatrixType { ty, rows, cols };
                Type::Numeric(NumericType::Matrix(matrix_type))
            }
            slang::TypeKind::Struct => {
                let field_count = slang_type_layout.field_count();
                let mut fields = Vec::with_capacity(field_count as usize);
                let mut i = 0;
                for slang_var_layout in slang_type_layout.fields() {
                    let binding_range_offset = slang_type_layout.field_binding_range_offset(i);
                    i += 1;

                    let var_layout =
                        self.build_var_layout(slang_var_layout, binding_range_offset)?;

                    let Some(field) = var_layout else {
                        return Err(anyhow::anyhow!("field not found"));
                    };

                    fields.push(field);
                }

                let name = slang_type_layout
                    .name()
                    .unwrap_or_default()
                    .to_compact_string();

                let struct_type = StructType { name, fields };
                Type::Struct(struct_type)
            }
            slang::TypeKind::Array => {
                let count = slang_type_layout
                    .element_count()
                    .context("array element count not found")?;

                let slang_element_type = slang_type_layout
                    .element_type_layout()
                    .context("array element type not found")?;

                let element = self
                    .build_type_layout(slang_element_type)?
                    .context("array element type not found")?;

                let array_type = ArrayType {
                    count: if count == usize::MAX {
                        ElementCount::Runtime
                    } else {
                        ElementCount::Bounded(count)
                    },
                    element: Box::new(element),
                };
                Type::Array(array_type)
            }
            slang::TypeKind::Resource => {
                let ty = slang_type_layout
                    .name()
                    .context("resource type name not found")?
                    .to_compact_string();

                let shape = slang_type_layout
                    .resource_shape()
                    .context("resource shape not found")?;

                let access = slang_type_layout.resource_access().map(ResourceAccess);

                // TODO: add context
                // let (set, binding) = self.get_current_vulkan_binding(slang_type_layout);
                // self.descriptors.insert(
                //     (set, binding),
                //     DescriptorMeta {
                //         shape,
                //         access: access.clone(),
                //     },
                // );

                let element = match slang_type_layout.element_type_layout() {
                    Some(element_type) => self.build_type_layout(element_type)?.map(Box::new),
                    None => None,
                };

                let resource_type = ResourceType {
                    ty,
                    binding: None,
                    shape,
                    access,
                    element,
                };
                Type::Resource(resource_type)
            }
            slang::TypeKind::SamplerState => {
                let name = slang_type_layout.name().unwrap_or_default();
                let sampler_state_type = SamplerStateType {
                    is_comparison_state: name == "SamplerComparisonState",
                };
                Type::SamplerState(sampler_state_type)
            }
            slang::TypeKind::ParameterBlock => {
                let slang_element_type = slang_type_layout
                    .element_type_layout()
                    .context("parameter block element type not found")?;

                let old_base_bytes = self.base_bytes;
                let old_base_binding_range = self.base_binding_range;

                // TODO: increase parameter block offset

                self.base_bytes = 0;
                self.base_binding_range = 0;

                let Some(element) = self.build_type_layout(slang_element_type)? else {
                    return Err(anyhow::anyhow!("element not found"));
                };

                let descriptor_set = self.get_binding_ranges(slang_element_type)?;

                self.base_bytes = old_base_bytes;
                self.base_binding_range = old_base_binding_range;

                let parameter_block_type = ParameterBlockType {
                    descriptor_set,
                    element: Box::new(element),
                };
                Type::ParameterBlock(parameter_block_type)
            }
            slang::TypeKind::ConstantBuffer => {
                let slang_element_type = slang_type_layout
                    .element_type_layout()
                    .context("constant buffer element type not found")?;

                let Some(element) = self.build_type_layout(slang_element_type)? else {
                    return Err(anyhow::anyhow!("element not found"));
                };

                Type::ConstantBuffer(Box::new(element))
            }
            _ => Type::Unknown(
                format!("{:?}", slang_type_layout.kind()),
                slang_type_layout
                    .name()
                    .unwrap_or_default()
                    .to_compact_string(),
            ),
        };

        let type_layout = TypeLayout {
            size,
            alignment,
            stride,
            ty,
        };

        Ok(Some(type_layout))
    }

    fn get_binding_ranges(
        &self,
        type_layout: &slang::reflection::TypeLayout,
    ) -> Result<DescriptorSet> {
        let mut set = DescriptorSet {
            set: -1,
            implicit_ubo: None,
            binding_ranges: vec![],
        };

        use BuilderLocation::*;
        let stages = match self.location {
            Global => vk::ShaderStageFlags::ALL,
            Entrypoint(SlangShaderStage::Vertex) => vk::ShaderStageFlags::VERTEX,
            Entrypoint(SlangShaderStage::Fragment) => vk::ShaderStageFlags::FRAGMENT,
            Entrypoint(SlangShaderStage::Compute) => vk::ShaderStageFlags::COMPUTE,
        };

        // implicit UBO binding
        if type_layout.size(slang::ParameterCategory::Uniform) > 0 {
            let set_index = type_layout.binding_range_descriptor_set_index(0);
            let vk_set = self.base_set as i64 + type_layout.descriptor_set_space_offset(set_index);

            if set.set == -1 {
                set.set = vk_set;
            } else {
                assert_eq!(set.set, vk_set);
            }

            set.implicit_ubo = Some(DescriptorBinding {
                binding: 0,
                stages,
                binding_type: slang::BindingType::ConstantBuffer,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                shape: None,  // TODO: what to use here?
                access: None, // TODO: ditto
                count: ElementCount::Bounded(1),
            });
        }

        let binding_range_count = type_layout.binding_range_count();
        for binding_range in 0..binding_range_count {
            let descriptor_count = type_layout.binding_range_descriptor_range_count(binding_range);
            if descriptor_count == 0 {
                // no descriptors in this binding range
                continue;
            }

            assert!(descriptor_count == 1, "should always be 1 for vulkan");

            let set_index = type_layout.binding_range_descriptor_set_index(binding_range);
            let vk_set = self.base_set as i64 + type_layout.descriptor_set_space_offset(set_index);

            if set.set == -1 {
                set.set = vk_set;
            } else {
                assert_eq!(set.set, vk_set);
            }

            let first = type_layout.binding_range_first_descriptor_range_index(binding_range);
            for i in 0..descriptor_count {
                let range_index = first + i;
                let vk_binding = type_layout
                    .descriptor_set_descriptor_range_index_offset(set_index, range_index);

                let binding_type =
                    type_layout.descriptor_set_descriptor_range_type(set_index, range_index);

                let vk_binding_type =
                    map_descriptor_type(binding_type)?.context("unexpected slang binding type")?;

                let vk_binding_count = type_layout
                    .descriptor_set_descriptor_range_descriptor_count(set_index, range_index);

                // if vk_binding_count < 0 -> runtime sized

                let descriptor_meta = self.descriptors.get(&(vk_set, vk_binding));

                let shape = descriptor_meta.map(|meta| ResourceShape(meta.shape));
                let access = descriptor_meta.map(|meta| meta.access).flatten();

                let binding_range = BindingRange {
                    range_index: binding_range,
                    descriptor: DescriptorBinding {
                        binding: vk_binding,
                        stages,
                        binding_type: binding_type,
                        descriptor_type: vk_binding_type,
                        shape,
                        access,
                        count: ElementCount::Bounded(vk_binding_count as usize),
                    },
                };

                set.binding_ranges.push(binding_range);
            }
        }

        Ok(set)
    }

    fn build_size(slang_type_layout: &slang::reflection::TypeLayout) -> Option<LayoutUnit> {
        let categories = slang_type_layout.categories();
        let mut size = LayoutUnit {
            push_constants: None,
            bytes: None,
            bindings: None,
            varying_input: None,
        };
        if categories.len() > 0 {
            for category in categories {
                use slang::ParameterCategory::*;
                match category {
                    PushConstantBuffer => {
                        size.push_constants = Some(slang_type_layout.size(category));
                    }
                    Uniform => {
                        size.bytes = Some(slang_type_layout.size(category));
                    }
                    DescriptorTableSlot => {
                        size.bindings = Some(slang_type_layout.size(category));
                    }
                    VaryingInput => {
                        size.varying_input = Some(slang_type_layout.size(category));
                    }
                    _ => {}
                }
            }
            Some(size)
        } else {
            None
        }
    }
}

fn map_descriptor_type(binding_type: slang::BindingType) -> Result<Option<vk::DescriptorType>> {
    use slang::BindingType::*;
    match binding_type {
        Sampler => Ok(Some(vk::DescriptorType::SAMPLER)),
        CombinedTextureSampler => Ok(Some(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)),
        Texture => Ok(Some(vk::DescriptorType::SAMPLED_IMAGE)),
        MutableTeture => Ok(Some(vk::DescriptorType::STORAGE_IMAGE)),
        TypedBuffer => Ok(Some(vk::DescriptorType::UNIFORM_TEXEL_BUFFER)),
        MutableTypedBuffer => Ok(Some(vk::DescriptorType::STORAGE_TEXEL_BUFFER)),
        RawBuffer => Ok(Some(vk::DescriptorType::STORAGE_BUFFER)),
        MutableRawBuffer => Ok(Some(vk::DescriptorType::STORAGE_BUFFER)),
        InputRenderTarget => Ok(Some(vk::DescriptorType::INPUT_ATTACHMENT)),
        InlineUniformData => Ok(Some(vk::DescriptorType::INLINE_UNIFORM_BLOCK)),
        RayTracingAccelerationStructure => Ok(Some(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)),
        ConstantBuffer => Ok(Some(vk::DescriptorType::UNIFORM_BUFFER)),
        PushConstant => Ok(None),
        _ => Err(anyhow!("unsupported binding type")),
    }
}
