//! Root reflection entry point.
//!
//! This module provides the main entry point for extracting layout information
//! from a compiled Slang program.

use std::collections::BTreeMap;

use anyhow::Result;
use compact_str::CompactString;
use shader_slang::{self as slang, ParameterCategory};
use vulkanalia::vk;

use super::layout::{DescriptorBindingLayout, DescriptorCount, DescriptorSetLayout, SlangLayout};
use super::reflect_type::reflect_type;
use super::types::SlangResource;

/// Extract complete layout information from a linked Slang program.
///
/// This walks the Slang reflection API and builds our high-level `SlangLayout`
/// structure that can be used to create Vulkan pipeline layouts.
pub fn reflect_layout(program: &slang::ComponentType) -> Result<SlangLayout> {
    // Get reflection data for target 0 (we only have one target: SPIR-V)
    let shader = program
        .layout(0)
        .map_err(|e| anyhow::anyhow!("failed to get program layout: {}", e))?;

    // Collector for descriptor bindings, keyed by (set, binding)
    let mut bindings: BTreeMap<(u32, u32), DescriptorBindingLayout> = BTreeMap::new();
    let mut stages = vk::ShaderStageFlags::empty();

    // Reflect global scope parameters
    if let Some(global_scope) = shader.global_params_var_layout() {
        reflect_scope(global_scope, &mut bindings, vk::ShaderStageFlags::ALL)?;
    }

    // Reflect entry point parameters
    for entry_point in shader.entry_points() {
        let stage_flag = stage_to_vk(entry_point.stage());
        stages |= stage_flag;

        if let Some(var_layout) = entry_point.var_layout() {
            reflect_scope(var_layout, &mut bindings, stage_flag)?;
        }
    }

    // Organize bindings into descriptor set layouts
    let descriptor_sets = organize_descriptor_sets(bindings);

    Ok(SlangLayout {
        bindless_heap: None,
        push_constants: None,
        descriptor_sets,
        parameter_blocks: Vec::new(),
        entrypoints: Vec::new(),
    })
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
    stages: vk::ShaderStageFlags,
) -> Result<()> {
    let Some(scope_type_layout) = scope_var_layout.type_layout() else {
        return Ok(());
    };

    // Handle automatic wrapping in ConstantBuffer or ParameterBlock
    match scope_type_layout.kind() {
        slang::TypeKind::ConstantBuffer => {
            // Global scope was wrapped in an implicit constant buffer
            // Container has the constant buffer binding, element has the parameters
            if let Some(element_var) = scope_type_layout.element_var_layout() {
                reflect_scope_parameters(element_var, bindings, stages)?;
            }
        }
        slang::TypeKind::ParameterBlock => {
            // Global scope was wrapped in an implicit parameter block
            // This means a whole descriptor set was allocated
            if let Some(element_var) = scope_type_layout.element_var_layout() {
                reflect_scope_parameters(element_var, bindings, stages)?;
            }
        }
        slang::TypeKind::Struct => {
            // Simple case: scope is just a struct of parameters
            reflect_scope_parameters(scope_var_layout, bindings, stages)?;
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
    stages: vk::ShaderStageFlags,
) -> Result<()> {
    let Some(scope_type_layout) = scope_var_layout.type_layout() else {
        return Ok(());
    };

    // Walk fields of the scope struct
    for field in scope_type_layout.fields() {
        reflect_parameter(field, bindings, stages)?;
    }

    Ok(())
}

/// Reflect a single parameter (could be a binding, struct, or nested container).
fn reflect_parameter(
    var_layout: &slang::reflection::VariableLayout,
    bindings: &mut BTreeMap<(u32, u32), DescriptorBindingLayout>,
    stages: vk::ShaderStageFlags,
) -> Result<()> {
    let Some(type_layout) = var_layout.type_layout() else {
        return Ok(());
    };

    let name = var_layout
        .name()
        .map(CompactString::from)
        .unwrap_or_default();

    // Check if this is a descriptor binding
    let has_binding = var_layout
        .categories()
        .any(|c| c == ParameterCategory::DescriptorTableSlot);

    if has_binding {
        // This is a descriptor binding
        let binding_index = var_layout.binding_index();
        let binding_space = var_layout.binding_space();

        // Try to convert to our resource type
        if let Ok(slang_type) = reflect_type(type_layout) {
            // Extract resource type - handle both direct resources and arrays of resources
            let (resource, count) = match &slang_type {
                super::types::SlangType::ResourceHandle(res) => {
                    (Some((**res).clone()), DescriptorCount::Count(1))
                }
                super::types::SlangType::Array(arr) => {
                    // Check if element is a resource
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

                // Merge with existing binding if present (for stage flags)
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

                // Don't recurse into resource types
                return Ok(());
            }
        }
    }

    // Recursively handle nested types
    match type_layout.kind() {
        slang::TypeKind::Struct => {
            for field in type_layout.fields() {
                reflect_parameter(field, bindings, stages)?;
            }
        }
        slang::TypeKind::ConstantBuffer | slang::TypeKind::ParameterBlock => {
            if let Some(element_var) = type_layout.element_var_layout() {
                reflect_parameter(element_var, bindings, stages)?;
            }
        }
        _ => {}
    }

    Ok(())
}

/// Extract a SlangResource from a SlangType, if it's a resource handle.
fn extract_resource(ty: &super::types::SlangType) -> Option<SlangResource> {
    match ty {
        super::types::SlangType::ResourceHandle(resource) => Some((**resource).clone()),
        _ => None,
    }
}
