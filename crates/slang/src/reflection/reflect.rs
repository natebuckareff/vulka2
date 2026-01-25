//! Root reflection entry point.
//!
//! This module provides the main entry point for extracting layout information
//! from a compiled Slang program.

use std::collections::BTreeMap;

use anyhow::Result;
use compact_str::CompactString;
use shader_slang::{self as slang, ParameterCategory};
use vulkanalia::vk;

use super::layout::{
    DescriptorBindingLayout, DescriptorCount, DescriptorSetLayout, OrdinaryParameterBinding,
    ParameterBlockLayout, ParameterBlockScope, PushConstantLayout, SlangLayout, SlangUnit,
};
use super::reflect_type::reflect_type;
use super::types::{
    BufferAccess, BufferElement, BufferKind, SlangBuffer, SlangResource, SlangStruct, SlangType,
};
use crate::compiler::SlangEntrypoint;

/// Cumulative offset tracker for access path traversal.
///
/// As we walk down through nested types, we accumulate offsets in multiple
/// dimensions (sets, bindings, bytes) to compute absolute positions.
#[derive(Debug, Clone, Default)]
struct AccessPath {
    /// Cumulative offset in layout units.
    offset: SlangUnit,
    /// The deepest parameter block encountered (resets binding/byte offsets).
    deepest_param_block_set: Option<u32>,
}

impl AccessPath {
    fn with_var_layout(&self, var_layout: &slang::reflection::VariableLayout) -> Self {
        Self {
            offset: SlangUnit {
                set_spaces: self.offset.set_spaces
                    + var_layout.offset(ParameterCategory::SubElementRegisterSpace) as u32,
                binding_slots: self.offset.binding_slots
                    + var_layout.offset(ParameterCategory::DescriptorTableSlot) as u32,
                bytes: self.offset.bytes + var_layout.offset(ParameterCategory::Uniform) as u32,
            },
            deepest_param_block_set: self.deepest_param_block_set,
        }
    }

    /// Enter a parameter block - resets binding offsets relative to new set.
    fn enter_param_block(&self, set: u32) -> Self {
        Self {
            offset: SlangUnit {
                set_spaces: set,
                binding_slots: 0,
                bytes: 0,
            },
            deepest_param_block_set: Some(set),
        }
    }

    fn current_set(&self) -> u32 {
        self.offset.set_spaces
    }

    fn current_binding(&self) -> u32 {
        self.offset.binding_slots
    }
}

/// Extract complete layout information from a linked Slang program.
///
/// This walks the Slang reflection API and builds our high-level `SlangLayout`
/// structure that can be used to create Vulkan pipeline layouts.
pub fn reflect_layout(program: &slang::ComponentType) -> Result<SlangLayout> {
    let shader = program
        .layout(0)
        .map_err(|e| anyhow::anyhow!("failed to get program layout: {}", e))?;

    // Collectors
    let mut bindings: BTreeMap<(u32, u32), DescriptorBindingLayout> = BTreeMap::new();
    let mut push_constant: Option<(SlangStruct, vk::ShaderStageFlags)> = None;
    let mut parameter_blocks: Vec<ParameterBlockLayout> = Vec::new();

    // Reflect global scope parameters
    if let Some(global_scope) = shader.global_params_var_layout() {
        let access_path = AccessPath::default();
        reflect_scope(
            global_scope,
            &mut bindings,
            &mut push_constant,
            &mut parameter_blocks,
            vk::ShaderStageFlags::ALL,
            ParameterBlockScope::Global,
            &access_path,
        )?;
    }

    // Reflect entry point parameters
    for entry_point in shader.entry_points() {
        let stage_flag = stage_to_vk(entry_point.stage());
        let entry_name = entry_point.name().unwrap_or_default();

        // Build entrypoint handle for scope identification
        let entrypoint = SlangEntrypoint::new(
            Default::default(), // TODO: proper module tracking
            CompactString::from(entry_name),
            slang_stage_to_our_stage(entry_point.stage()),
        );

        if let Some(var_layout) = entry_point.var_layout() {
            // Start access path with the entry point's base offsets
            let access_path = AccessPath {
                offset: SlangUnit {
                    set_spaces: var_layout.offset(ParameterCategory::SubElementRegisterSpace)
                        as u32,
                    binding_slots: var_layout.offset(ParameterCategory::DescriptorTableSlot) as u32,
                    bytes: var_layout.offset(ParameterCategory::Uniform) as u32,
                },
                deepest_param_block_set: None,
            };

            reflect_scope(
                var_layout,
                &mut bindings,
                &mut push_constant,
                &mut parameter_blocks,
                stage_flag,
                ParameterBlockScope::Entrypoint(entrypoint),
                &access_path,
            )?;
        }
    }

    // Organize bindings into descriptor set layouts
    let descriptor_sets = organize_descriptor_sets(bindings);

    // Build push constant layout
    let push_constants = push_constant.map(|(ty, stages)| PushConstantLayout {
        stages,
        size_bytes: ty.layout.size.bytes,
        ty,
    });

    Ok(SlangLayout {
        bindless_heap: None,
        push_constants,
        descriptor_sets,
        parameter_blocks,
        entrypoints: Vec::new(),
    })
}

fn slang_stage_to_our_stage(stage: slang::Stage) -> crate::compiler::SlangShaderStage {
    match stage {
        slang::Stage::Vertex => crate::compiler::SlangShaderStage::Vertex,
        slang::Stage::Fragment => crate::compiler::SlangShaderStage::Fragment,
        slang::Stage::Compute => crate::compiler::SlangShaderStage::Compute,
        _ => crate::compiler::SlangShaderStage::Compute, // fallback
    }
}

/// Organize collected bindings into DescriptorSetLayout structures.
fn organize_descriptor_sets(
    bindings: BTreeMap<(u32, u32), DescriptorBindingLayout>,
) -> Vec<DescriptorSetLayout> {
    let mut sets: BTreeMap<u32, Vec<DescriptorBindingLayout>> = BTreeMap::new();

    for ((set, _binding), layout) in bindings {
        sets.entry(set).or_default().push(layout);
    }

    sets.into_iter()
        .map(|(set, bindings)| DescriptorSetLayout { set, bindings })
        .collect()
}

/// Convert Slang stage to Vulkan stage flag.
fn stage_to_vk(stage: slang::Stage) -> vk::ShaderStageFlags {
    match stage {
        slang::Stage::Vertex => vk::ShaderStageFlags::VERTEX,
        slang::Stage::Fragment => vk::ShaderStageFlags::FRAGMENT,
        slang::Stage::Compute => vk::ShaderStageFlags::COMPUTE,
        slang::Stage::Geometry => vk::ShaderStageFlags::GEOMETRY,
        slang::Stage::Hull => vk::ShaderStageFlags::TESSELLATION_CONTROL,
        slang::Stage::Domain => vk::ShaderStageFlags::TESSELLATION_EVALUATION,
        _ => vk::ShaderStageFlags::ALL,
    }
}

/// Reflect parameters from a scope (global or entry point).
fn reflect_scope(
    scope_var_layout: &slang::reflection::VariableLayout,
    bindings: &mut BTreeMap<(u32, u32), DescriptorBindingLayout>,
    push_constant: &mut Option<(SlangStruct, vk::ShaderStageFlags)>,
    parameter_blocks: &mut Vec<ParameterBlockLayout>,
    stages: vk::ShaderStageFlags,
    scope: ParameterBlockScope,
    access_path: &AccessPath,
) -> Result<()> {
    let Some(scope_type_layout) = scope_var_layout.type_layout() else {
        return Ok(());
    };

    // Handle automatic wrapping in ConstantBuffer or ParameterBlock
    match scope_type_layout.kind() {
        slang::TypeKind::ConstantBuffer => {
            // Global scope was wrapped in an implicit constant buffer
            if let Some(element_var) = scope_type_layout.element_var_layout() {
                let inner_path = access_path.with_var_layout(element_var);
                reflect_scope_parameters(
                    element_var,
                    bindings,
                    push_constant,
                    parameter_blocks,
                    stages,
                    scope,
                    &inner_path,
                )?;
            }
        }
        slang::TypeKind::ParameterBlock => {
            // Global scope was wrapped in an implicit parameter block
            if let Some(element_var) = scope_type_layout.element_var_layout() {
                let inner_path = access_path.with_var_layout(element_var);
                reflect_scope_parameters(
                    element_var,
                    bindings,
                    push_constant,
                    parameter_blocks,
                    stages,
                    scope,
                    &inner_path,
                )?;
            }
        }
        slang::TypeKind::Struct => {
            // Simple case: scope is just a struct of parameters
            reflect_scope_parameters(
                scope_var_layout,
                bindings,
                push_constant,
                parameter_blocks,
                stages,
                scope,
                access_path,
            )?;
        }
        _ => {
            // Unexpected scope type
        }
    }

    Ok(())
}

/// Reflect the parameters within a scope.
fn reflect_scope_parameters(
    scope_var_layout: &slang::reflection::VariableLayout,
    bindings: &mut BTreeMap<(u32, u32), DescriptorBindingLayout>,
    push_constant: &mut Option<(SlangStruct, vk::ShaderStageFlags)>,
    parameter_blocks: &mut Vec<ParameterBlockLayout>,
    stages: vk::ShaderStageFlags,
    scope: ParameterBlockScope,
    access_path: &AccessPath,
) -> Result<()> {
    let Some(scope_type_layout) = scope_var_layout.type_layout() else {
        return Ok(());
    };

    // Walk fields of the scope struct
    for field in scope_type_layout.fields() {
        reflect_parameter(
            field,
            bindings,
            push_constant,
            parameter_blocks,
            stages,
            &scope,
            access_path,
        )?;
    }

    Ok(())
}

/// Reflect a single parameter (could be a binding, struct, or nested container).
fn reflect_parameter(
    var_layout: &slang::reflection::VariableLayout,
    bindings: &mut BTreeMap<(u32, u32), DescriptorBindingLayout>,
    push_constant: &mut Option<(SlangStruct, vk::ShaderStageFlags)>,
    parameter_blocks: &mut Vec<ParameterBlockLayout>,
    stages: vk::ShaderStageFlags,
    scope: &ParameterBlockScope,
    access_path: &AccessPath,
) -> Result<()> {
    let Some(type_layout) = var_layout.type_layout() else {
        return Ok(());
    };

    let name = var_layout
        .name()
        .map(CompactString::from)
        .unwrap_or_default();

    // Check if this is a push constant
    let is_push_constant = var_layout
        .categories()
        .any(|c| c == ParameterCategory::PushConstantBuffer);

    if is_push_constant {
        let type_to_reflect = if type_layout.kind() == slang::TypeKind::ConstantBuffer {
            type_layout.element_type_layout().unwrap_or(type_layout)
        } else {
            type_layout
        };

        if let Ok(SlangType::Struct(struct_type)) = reflect_type(type_to_reflect) {
            if let Some((_, existing_stages)) = push_constant {
                *existing_stages |= stages;
            } else {
                *push_constant = Some((struct_type, stages));
            }
        }
        return Ok(());
    }

    // Check if this is a ParameterBlock
    if type_layout.kind() == slang::TypeKind::ParameterBlock {
        reflect_parameter_block(
            var_layout,
            type_layout,
            &name,
            bindings,
            parameter_blocks,
            stages,
            scope,
            access_path,
        )?;
        return Ok(());
    }

    // Check if this is a descriptor binding
    let has_binding = var_layout
        .categories()
        .any(|c| c == ParameterCategory::DescriptorTableSlot);

    if has_binding {
        let inner_path = access_path.with_var_layout(var_layout);
        let binding_index = inner_path.current_binding();
        let binding_space = inner_path.current_set();

        if let Ok(slang_type) = reflect_type(type_layout) {
            let (resource, count) = match &slang_type {
                SlangType::ResourceHandle(res) => {
                    (Some((**res).clone()), DescriptorCount::Count(1))
                }
                SlangType::Array(arr) => {
                    if let Some(res) = extract_resource(&arr.element_type) {
                        let count = if arr.element_count == 0 || arr.element_count == u32::MAX {
                            DescriptorCount::Variable
                        } else {
                            DescriptorCount::Count(arr.element_count)
                        };
                        (Some(res), count)
                    } else {
                        (None, DescriptorCount::Count(1))
                    }
                }
                _ => (None, DescriptorCount::Count(1)),
            };

            if let Some(resource) = resource {
                let key = (binding_space, binding_index);

                if let Some(existing) = bindings.get_mut(&key) {
                    existing.stages |= stages;
                } else {
                    bindings.insert(
                        key,
                        DescriptorBindingLayout {
                            binding: binding_index,
                            name,
                            flags: vk::DescriptorBindingFlags::empty(),
                            stages,
                            ty: resource,
                            count,
                        },
                    );
                }
                return Ok(());
            }
        }
    }

    // Recursively handle nested types
    let inner_path = access_path.with_var_layout(var_layout);
    match type_layout.kind() {
        slang::TypeKind::Struct => {
            for field in type_layout.fields() {
                reflect_parameter(
                    field,
                    bindings,
                    push_constant,
                    parameter_blocks,
                    stages,
                    scope,
                    &inner_path,
                )?;
            }
        }
        slang::TypeKind::ConstantBuffer => {
            if let Some(element_var) = type_layout.element_var_layout() {
                reflect_parameter(
                    element_var,
                    bindings,
                    push_constant,
                    parameter_blocks,
                    stages,
                    scope,
                    &inner_path,
                )?;
            }
        }
        _ => {}
    }

    Ok(())
}

/// Reflect a ParameterBlock parameter.
fn reflect_parameter_block(
    var_layout: &slang::reflection::VariableLayout,
    type_layout: &slang::reflection::TypeLayout,
    name: &CompactString,
    bindings: &mut BTreeMap<(u32, u32), DescriptorBindingLayout>,
    parameter_blocks: &mut Vec<ParameterBlockLayout>,
    stages: vk::ShaderStageFlags,
    scope: &ParameterBlockScope,
    access_path: &AccessPath,
) -> Result<()> {
    // Get the descriptor set allocated for this parameter block
    let set_offset = var_layout.offset(ParameterCategory::SubElementRegisterSpace) as u32;
    let absolute_set = access_path.current_set() + set_offset;

    // Get container var layout (holds the implicit constant buffer, if any)
    let container_var = type_layout.container_var_layout();

    // Get element var layout (holds T's fields)
    let element_var = type_layout.element_var_layout();
    let element_type_layout = element_var.and_then(|e| e.type_layout());

    // Reflect the element type
    let ty = element_type_layout
        .and_then(|t| reflect_type(t).ok())
        .unwrap_or(SlangType::Struct(SlangStruct {
            name: name.clone(),
            layout: Default::default(),
            fields: Vec::new(),
        }));

    // Create access path inside this parameter block (for nested param blocks)
    let _inner_path = access_path.enter_param_block(absolute_set);

    // Collect bindings within this parameter block
    let mut block_bindings: Vec<DescriptorBindingLayout> = Vec::new();

    // Check if there's an implicit constant buffer from the container
    // The container's binding tells us where the implicit const buffer lives
    let container_binding = container_var
        .map(|c| c.offset(ParameterCategory::DescriptorTableSlot) as u32)
        .unwrap_or(0);

    let container_has_binding = container_var
        .and_then(|c| c.type_layout())
        .map(|t| t.size(ParameterCategory::DescriptorTableSlot) > 0)
        .unwrap_or(false);

    // If the element has ordinary (byte) data, we have an implicit constant buffer
    let ordinary = if let Some(elem_type_layout) = element_type_layout {
        let uniform_size = elem_type_layout.size(ParameterCategory::Uniform);
        if uniform_size > 0 && container_has_binding {
            // Extract the struct type for ordinary data
            let ordinary_struct = if let SlangType::Struct(s) = &ty {
                s.clone()
            } else {
                SlangStruct {
                    name: name.clone(),
                    layout: Default::default(),
                    fields: Vec::new(),
                }
            };

            // Add to flat descriptor set bindings
            let key = (absolute_set, container_binding);
            if let Some(existing) = bindings.get_mut(&key) {
                existing.stages |= stages;
            } else {
                bindings.insert(
                    key,
                    DescriptorBindingLayout {
                        binding: container_binding,
                        name: name.clone(),
                        flags: vk::DescriptorBindingFlags::empty(),
                        stages,
                        ty: SlangResource::Buffer(SlangBuffer {
                            kind: BufferKind::Uniform,
                            access: BufferAccess::ReadOnly,
                            element: BufferElement::Typed(ty.clone()),
                            block_alignment_bytes: 0,
                            trailing_array: None,
                        }),
                        count: DescriptorCount::Count(1),
                    },
                );
            }

            Some(OrdinaryParameterBinding {
                binding: container_binding,
                ty: ordinary_struct,
            })
        } else {
            None
        }
    } else {
        None
    };

    // Get the element's binding offset (where T's fields start)
    // This accounts for the implicit constant buffer taking binding 0
    let element_binding_offset = element_var
        .map(|e| e.offset(ParameterCategory::DescriptorTableSlot) as u32)
        .unwrap_or(0);

    // Reflect fields within the element
    if let Some(elem_var) = element_var {
        if let Some(elem_type) = elem_var.type_layout() {
            for field in elem_type.fields() {
                let field_name = field.name().map(CompactString::from).unwrap_or_default();

                let has_binding = field
                    .categories()
                    .any(|c| c == ParameterCategory::DescriptorTableSlot);

                if has_binding {
                    // Field binding = element's base binding offset + field's relative binding
                    let field_binding = element_binding_offset
                        + field.offset(ParameterCategory::DescriptorTableSlot) as u32;

                    if let Some(field_type) = field.type_layout() {
                        if let Ok(slang_type) = reflect_type(field_type) {
                            let (resource, count) = match &slang_type {
                                SlangType::ResourceHandle(res) => {
                                    (Some((**res).clone()), DescriptorCount::Count(1))
                                }
                                SlangType::Array(arr) => {
                                    if let Some(res) = extract_resource(&arr.element_type) {
                                        let count = if arr.element_count == 0
                                            || arr.element_count == u32::MAX
                                        {
                                            DescriptorCount::Variable
                                        } else {
                                            DescriptorCount::Count(arr.element_count)
                                        };
                                        (Some(res), count)
                                    } else {
                                        (None, DescriptorCount::Count(1))
                                    }
                                }
                                _ => (None, DescriptorCount::Count(1)),
                            };

                            if let Some(resource) = resource {
                                // Add to block-local bindings
                                block_bindings.push(DescriptorBindingLayout {
                                    binding: field_binding,
                                    name: field_name.clone(),
                                    flags: vk::DescriptorBindingFlags::empty(),
                                    stages,
                                    ty: resource.clone(),
                                    count,
                                });

                                // Also add to flat descriptor set bindings
                                let key = (absolute_set, field_binding);
                                if let Some(existing) = bindings.get_mut(&key) {
                                    existing.stages |= stages;
                                } else {
                                    bindings.insert(
                                        key,
                                        DescriptorBindingLayout {
                                            binding: field_binding,
                                            name: field_name,
                                            flags: vk::DescriptorBindingFlags::empty(),
                                            stages,
                                            ty: resource,
                                            count,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Build the ParameterBlockLayout
    parameter_blocks.push(ParameterBlockLayout {
        scope: scope.clone(),
        name: name.clone(),
        ty,
        set: absolute_set,
        ordinary,
        bindings: block_bindings,
        nested: Vec::new(), // TODO: handle nested parameter blocks
    });

    Ok(())
}

/// Extract a SlangResource from a SlangType, if it's a resource handle.
fn extract_resource(ty: &SlangType) -> Option<SlangResource> {
    match ty {
        SlangType::ResourceHandle(resource) => Some((**resource).clone()),
        _ => None,
    }
}
