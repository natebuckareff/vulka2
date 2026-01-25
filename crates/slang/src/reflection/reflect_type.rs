//! Type conversion from Slang reflection API to our types.

use anyhow::{Result, bail};
use compact_str::CompactString;
use shader_slang::{self as slang, ParameterCategory, ResourceAccess, ResourceShape, TypeKind};

use super::layout::SlangUnit;
use super::types::*;

/// Convert a Slang TypeLayout to our SlangType.
pub fn reflect_type(type_layout: &slang::reflection::TypeLayout) -> Result<SlangType> {
    let kind = type_layout.kind();

    match kind {
        TypeKind::Scalar => reflect_scalar(type_layout),
        TypeKind::Vector => reflect_vector(type_layout),
        TypeKind::Matrix => reflect_matrix(type_layout),
        TypeKind::Struct => reflect_struct(type_layout),
        TypeKind::Array => reflect_array(type_layout),
        TypeKind::Resource => reflect_resource(type_layout),
        TypeKind::SamplerState => Ok(SlangType::ResourceHandle(Box::new(SlangResource::Sampler))),
        TypeKind::ConstantBuffer | TypeKind::ParameterBlock => {
            // For containers, reflect the element type
            if let Some(element_var) = type_layout.element_var_layout() {
                if let Some(element_type) = element_var.type_layout() {
                    return reflect_type(element_type);
                }
            }
            bail!("failed to get element type from container")
        }
        TypeKind::Pointer => {
            // Pointer types in Slang are GPU buffer device addresses (BDA)
            Ok(SlangType::DeviceAddress)
        }
        _ => {
            // For unknown types, try to get the name and bail
            let name = type_layout.name().unwrap_or("<unknown>");
            bail!("unsupported type kind {:?} for '{}'", kind, name)
        }
    }
}

/// Extract TypeLayout (size, alignment, stride) from a Slang TypeLayout.
pub fn extract_type_layout(type_layout: &slang::reflection::TypeLayout) -> TypeLayout {
    TypeLayout {
        size: SlangUnit {
            bytes: type_layout.size(ParameterCategory::Uniform) as u32,
            binding_slots: type_layout.size(ParameterCategory::DescriptorTableSlot) as u32,
            set_spaces: type_layout.size(ParameterCategory::SubElementRegisterSpace) as u32,
        },
        alignment_bytes: type_layout.alignment(ParameterCategory::Uniform).max(0) as u32,
        stride: SlangUnit {
            bytes: type_layout.stride(ParameterCategory::Uniform) as u32,
            binding_slots: type_layout.stride(ParameterCategory::DescriptorTableSlot) as u32,
            set_spaces: type_layout.stride(ParameterCategory::SubElementRegisterSpace) as u32,
        },
    }
}

/// Convert Slang ScalarType to our ScalarKind.
fn convert_scalar_kind(scalar_type: slang::ScalarType) -> Result<ScalarKind> {
    // ScalarType values from slang-sys (note mixed case: Uint32 not UInt32)
    match scalar_type {
        slang::ScalarType::Bool => Ok(ScalarKind::Bool),
        slang::ScalarType::Int32 => Ok(ScalarKind::Int32),
        slang::ScalarType::Uint32 => Ok(ScalarKind::UInt32),
        slang::ScalarType::Float32 => Ok(ScalarKind::Float32),
        // Handle other scalar types by mapping to closest match
        slang::ScalarType::Int64 | slang::ScalarType::Int16 | slang::ScalarType::Int8 => {
            Ok(ScalarKind::Int32)
        }
        slang::ScalarType::Uint64 | slang::ScalarType::Uint16 | slang::ScalarType::Uint8 => {
            Ok(ScalarKind::UInt32)
        }
        slang::ScalarType::Float64 | slang::ScalarType::Float16 => Ok(ScalarKind::Float32),
        _ => bail!("unsupported scalar type {:?}", scalar_type),
    }
}

fn reflect_scalar(type_layout: &slang::reflection::TypeLayout) -> Result<SlangType> {
    let scalar_type = type_layout
        .scalar_type()
        .ok_or_else(|| anyhow::anyhow!("failed to get scalar type"))?;
    let kind = convert_scalar_kind(scalar_type)?;
    let layout = extract_type_layout(type_layout);

    Ok(SlangType::Scalar(SlangScalar { kind, layout }))
}

fn reflect_vector(type_layout: &slang::reflection::TypeLayout) -> Result<SlangType> {
    let ty = type_layout
        .ty()
        .ok_or_else(|| anyhow::anyhow!("failed to get type for vector"))?;

    let scalar_type = ty.scalar_type();
    let scalar = convert_scalar_kind(scalar_type)?;
    let count = ty.element_count() as u32;
    let layout = extract_type_layout(type_layout);

    Ok(SlangType::Vector(SlangVector {
        scalar,
        count,
        layout,
    }))
}

fn reflect_matrix(type_layout: &slang::reflection::TypeLayout) -> Result<SlangType> {
    let ty = type_layout
        .ty()
        .ok_or_else(|| anyhow::anyhow!("failed to get type for matrix"))?;

    let scalar_type = ty.scalar_type();
    let scalar = convert_scalar_kind(scalar_type)?;
    let rows = ty.row_count();
    let cols = ty.column_count();
    let layout = extract_type_layout(type_layout);

    Ok(SlangType::Matrix(SlangMatrix {
        scalar,
        rows,
        cols,
        layout,
    }))
}

fn reflect_struct(type_layout: &slang::reflection::TypeLayout) -> Result<SlangType> {
    let name = type_layout
        .name()
        .map(CompactString::from)
        .unwrap_or_default();
    let layout = extract_type_layout(type_layout);

    let mut fields = Vec::new();
    for field_var in type_layout.fields() {
        let field_name = field_var
            .name()
            .map(CompactString::from)
            .unwrap_or_default();

        let field_type_layout = field_var
            .type_layout()
            .ok_or_else(|| anyhow::anyhow!("failed to get type layout for field '{}'", field_name))?;

        let offset = SlangUnit {
            bytes: field_var.offset(ParameterCategory::Uniform) as u32,
            binding_slots: field_var.offset(ParameterCategory::DescriptorTableSlot) as u32,
            set_spaces: field_var.offset(ParameterCategory::SubElementRegisterSpace) as u32,
        };

        let field_layout = extract_type_layout(field_type_layout);
        let ty = reflect_type(field_type_layout)?;

        fields.push(SlangField {
            name: field_name,
            offset,
            layout: field_layout,
            ty,
        });
    }

    Ok(SlangType::Struct(SlangStruct {
        name,
        layout,
        fields,
    }))
}

fn reflect_array(type_layout: &slang::reflection::TypeLayout) -> Result<SlangType> {
    let element_count = type_layout.element_count().unwrap_or(0) as u32;
    let layout = extract_type_layout(type_layout);

    let element_type_layout = type_layout
        .element_type_layout()
        .ok_or_else(|| anyhow::anyhow!("failed to get element type for array"))?;

    let element_type = reflect_type(element_type_layout)?;

    Ok(SlangType::Array(SlangArray {
        layout,
        element_count,
        element_type: Box::new(element_type),
    }))
}

fn reflect_resource(type_layout: &slang::reflection::TypeLayout) -> Result<SlangType> {
    let ty = type_layout
        .ty()
        .ok_or_else(|| anyhow::anyhow!("failed to get type for resource"))?;

    let shape = ty.resource_shape();
    let access = ty.resource_access();

    let resource = match shape {
        // Texture types (note: SlangTexture1d, not Texture1D)
        ResourceShape::SlangTexture1d
        | ResourceShape::SlangTexture2d
        | ResourceShape::SlangTexture3d
        | ResourceShape::SlangTextureCube
        | ResourceShape::SlangTexture1dArray
        | ResourceShape::SlangTexture2dArray
        | ResourceShape::SlangTextureCubeArray
        | ResourceShape::SlangTexture2dMultisample
        | ResourceShape::SlangTexture2dMultisampleArray => {
            reflect_texture(ty, shape, access)?
        }

        // Buffer types
        ResourceShape::SlangStructuredBuffer | ResourceShape::SlangByteAddressBuffer => {
            reflect_buffer(type_layout, ty, shape, access)?
        }

        _ => {
            bail!("unsupported resource shape {:?}", shape)
        }
    };

    Ok(SlangType::ResourceHandle(Box::new(resource)))
}

fn reflect_texture(
    ty: &slang::reflection::Type,
    shape: ResourceShape,
    access: ResourceAccess,
) -> Result<SlangResource> {
    let (dim, array, multisampled) = match shape {
        ResourceShape::SlangTexture1d => (TextureDim::One, false, false),
        ResourceShape::SlangTexture2d => (TextureDim::Two, false, false),
        ResourceShape::SlangTexture3d => (TextureDim::Three, false, false),
        ResourceShape::SlangTextureCube => (TextureDim::Cube, false, false),
        ResourceShape::SlangTexture1dArray => (TextureDim::One, true, false),
        ResourceShape::SlangTexture2dArray => (TextureDim::Two, true, false),
        ResourceShape::SlangTextureCubeArray => (TextureDim::Cube, true, false),
        ResourceShape::SlangTexture2dMultisample => (TextureDim::Two, false, true),
        ResourceShape::SlangTexture2dMultisampleArray => (TextureDim::Two, true, true),
        _ => bail!("unsupported texture shape {:?}", shape),
    };

    let tex_access = match access {
        ResourceAccess::Read => TextureAccess::ReadOnly,
        ResourceAccess::ReadWrite => TextureAccess::ReadWrite,
        _ => TextureAccess::ReadOnly,
    };

    // Get the sampled type from the resource result type
    let sampled = if let Some(result_type) = ty.resource_result_type() {
        reflect_sampled_type(result_type)?
    } else {
        // Default to float4
        SampledType::Vector {
            scalar: ScalarKind::Float32,
            count: 4,
        }
    };

    Ok(SlangResource::Texture(SlangTexture {
        dim,
        array,
        multisampled,
        access: tex_access,
        sampled,
    }))
}

fn reflect_sampled_type(result_type: &slang::reflection::Type) -> Result<SampledType> {
    match result_type.kind() {
        TypeKind::Scalar => {
            let scalar = convert_scalar_kind(result_type.scalar_type())?;
            Ok(SampledType::Scalar(scalar))
        }
        TypeKind::Vector => {
            let scalar = convert_scalar_kind(result_type.scalar_type())?;
            let count = result_type.element_count() as u32;
            Ok(SampledType::Vector { scalar, count })
        }
        _ => {
            // Default fallback
            Ok(SampledType::Vector {
                scalar: ScalarKind::Float32,
                count: 4,
            })
        }
    }
}

fn reflect_buffer(
    type_layout: &slang::reflection::TypeLayout,
    ty: &slang::reflection::Type,
    shape: ResourceShape,
    access: ResourceAccess,
) -> Result<SlangResource> {
    let kind = match shape {
        ResourceShape::SlangByteAddressBuffer => BufferKind::Storage,
        ResourceShape::SlangStructuredBuffer => BufferKind::Storage,
        _ => BufferKind::Storage,
    };

    let buf_access = match access {
        ResourceAccess::Read => BufferAccess::ReadOnly,
        ResourceAccess::ReadWrite => BufferAccess::ReadWrite,
        _ => BufferAccess::ReadOnly,
    };

    let element = if shape == ResourceShape::SlangByteAddressBuffer {
        BufferElement::RawBytes
    } else if let Some(result_type) = ty.resource_result_type() {
        // Get the element type layout if available
        if let Some(element_layout) = type_layout.element_type_layout() {
            let element_type = reflect_type(element_layout)?;
            BufferElement::Typed(element_type)
        } else {
            // Fall back to reflecting the result type without layout
            let result_kind = result_type.kind();
            match result_kind {
                TypeKind::Scalar => {
                    let scalar = convert_scalar_kind(result_type.scalar_type())?;
                    BufferElement::Typed(SlangType::Scalar(SlangScalar {
                        kind: scalar,
                        layout: TypeLayout::default(),
                    }))
                }
                _ => BufferElement::RawBytes,
            }
        }
    } else {
        BufferElement::RawBytes
    };

    let block_alignment_bytes = type_layout.alignment(ParameterCategory::Uniform).max(0) as u32;

    Ok(SlangResource::Buffer(SlangBuffer {
        kind,
        access: buf_access,
        element,
        block_alignment_bytes,
        trailing_array: None, // TODO: detect trailing unsized arrays
    }))
}

