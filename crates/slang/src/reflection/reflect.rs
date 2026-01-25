//! Root reflection entry point.
//!
//! This module provides the main entry point for extracting layout information
//! from a compiled Slang program.

use anyhow::Result;
use shader_slang::{self as slang, ParameterCategory};

use super::layout::SlangLayout;

/// Extract complete layout information from a linked Slang program.
///
/// This walks the Slang reflection API and builds our high-level `SlangLayout`
/// structure that can be used to create Vulkan pipeline layouts.
pub fn reflect_layout(program: &slang::ComponentType) -> Result<SlangLayout> {
    // Get reflection data for target 0 (we only have one target: SPIR-V)
    let shader = program
        .layout(0)
        .map_err(|e| anyhow::anyhow!("failed to get program layout: {}", e))?;

    // Start with an empty layout
    let mut layout = SlangLayout::default();

    // Reflect global scope parameters
    if let Some(global_scope) = shader.global_params_var_layout() {
        reflect_scope(global_scope, &mut layout)?;
    }

    // Reflect entry point parameters
    for entry_point in shader.entry_points() {
        reflect_entry_point(entry_point, &mut layout)?;
    }

    Ok(layout)
}

/// Reflect parameters from a scope (global or entry point).
fn reflect_scope(
    scope_var_layout: &slang::reflection::VariableLayout,
    layout: &mut SlangLayout,
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
                reflect_scope_parameters(element_var, layout)?;
            }
        }
        slang::TypeKind::ParameterBlock => {
            // Global scope was wrapped in an implicit parameter block
            // This means a whole descriptor set was allocated
            if let Some(element_var) = scope_type_layout.element_var_layout() {
                reflect_scope_parameters(element_var, layout)?;
            }
        }
        slang::TypeKind::Struct => {
            // Simple case: scope is just a struct of parameters
            reflect_scope_parameters(scope_var_layout, layout)?;
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
    layout: &mut SlangLayout,
) -> Result<()> {
    let Some(scope_type_layout) = scope_var_layout.type_layout() else {
        return Ok(());
    };

    // Walk fields of the scope struct
    for field in scope_type_layout.fields() {
        reflect_parameter(field, layout)?;
    }

    Ok(())
}

/// Reflect a single parameter (could be a binding, struct, or nested container).
fn reflect_parameter(
    var_layout: &slang::reflection::VariableLayout,
    layout: &mut SlangLayout,
) -> Result<()> {
    let Some(type_layout) = var_layout.type_layout() else {
        return Ok(());
    };

    let _name = var_layout.name().unwrap_or("<anonymous>");

    // Check what categories this parameter uses
    for category in var_layout.categories() {
        match category {
            ParameterCategory::DescriptorTableSlot => {
                // This is a descriptor binding
                let binding_index = var_layout.binding_index();
                let binding_space = var_layout.binding_space();

                // TODO: Convert type to SlangResource and add to layout
                let _ = (binding_index, binding_space, type_layout);
            }
            ParameterCategory::Uniform => {
                // Ordinary data (bytes) - part of push constants or uniform buffer
                let _byte_offset = var_layout.offset(ParameterCategory::Uniform);
                let _byte_size = type_layout.size(ParameterCategory::Uniform);
            }
            ParameterCategory::SubElementRegisterSpace => {
                // Uses a whole descriptor set
                let _space = var_layout.offset(ParameterCategory::SubElementRegisterSpace);
            }
            _ => {
                // Other categories (varying input/output, etc.)
            }
        }
    }

    // Recursively handle nested types
    match type_layout.kind() {
        slang::TypeKind::Struct => {
            for field in type_layout.fields() {
                reflect_parameter(field, layout)?;
            }
        }
        slang::TypeKind::ConstantBuffer | slang::TypeKind::ParameterBlock => {
            if let Some(element_var) = type_layout.element_var_layout() {
                reflect_parameter(element_var, layout)?;
            }
        }
        slang::TypeKind::Array => {
            // Arrays of resources need special handling
            // TODO: handle array bindings
        }
        _ => {}
    }

    Ok(())
}

/// Reflect an entry point's parameters and stage-specific information.
fn reflect_entry_point(
    entry_point: &slang::reflection::EntryPoint,
    layout: &mut SlangLayout,
) -> Result<()> {
    let _stage = entry_point.stage();
    let _name = entry_point.name();

    // Reflect entry point parameters (similar to global scope)
    if let Some(var_layout) = entry_point.var_layout() {
        reflect_scope(var_layout, layout)?;
    }

    // TODO: For vertex shaders, extract vertex input attributes
    // TODO: For compute shaders, extract thread group size

    Ok(())
}
