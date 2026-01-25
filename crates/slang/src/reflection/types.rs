use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use vulkanalia::vk;

use super::SlangUnit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct TypeLayout {
    pub size: SlangUnit,
    pub alignment_bytes: u32,
    pub stride: SlangUnit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlangType {
    Scalar(SlangScalar),
    Vector(SlangVector),
    Matrix(SlangMatrix),
    Struct(SlangStruct),
    Array(SlangArray),

    /// A bare opaque resource type within a ParameterBlock struct.
    ///
    /// When a `Texture2D`, `SamplerState`, etc. appears as a field in a struct
    /// used with `ParameterBlock<T>`, Slang automatically allocates descriptor
    /// bindings for it. This variant represents such fields in the type tree.
    ResourceHandle(Box<SlangResource>),

    /// A bindless heap handle (`Texture2D.Handle` / `DescriptorHandle<T>`).
    ///
    /// These are integer-like handles that index into a global bindless
    /// descriptor heap. They appear as ordinary data (u32/u64) in the struct
    /// layout, not as descriptor bindings.
    BindlessHandle(Box<SlangResource>),

    DeviceAddress,
}

impl SlangType {
    /// Get the layout information for this type.
    ///
    /// Returns `None` for opaque resource handles (ResourceHandle, BindlessHandle)
    /// which don't have a byte layout in the traditional sense.
    pub fn layout(&self) -> Option<&TypeLayout> {
        match self {
            SlangType::Scalar(s) => Some(&s.layout),
            SlangType::Vector(v) => Some(&v.layout),
            SlangType::Matrix(m) => Some(&m.layout),
            SlangType::Struct(s) => Some(&s.layout),
            SlangType::Array(a) => Some(&a.layout),
            SlangType::ResourceHandle(_) => None,
            SlangType::BindlessHandle(_) => None,
            SlangType::DeviceAddress => None,
        }
    }

    pub fn size_bytes(&self) -> Option<u32> {
        match self {
            SlangType::DeviceAddress => Some(8),
            SlangType::BindlessHandle(_) => Some(4), // Bindless handles are u32 indices
            _ => self.layout().map(|l| l.size.bytes),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScalarKind {
    Bool,
    Int32,
    UInt32,
    Float32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlangScalar {
    pub kind: ScalarKind,
    pub layout: TypeLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlangVector {
    pub scalar: ScalarKind,
    pub count: u32,
    pub layout: TypeLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlangMatrix {
    pub scalar: ScalarKind,
    pub rows: u32,
    pub cols: u32,
    pub layout: TypeLayout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlangStruct {
    pub name: CompactString,
    pub layout: TypeLayout,
    pub fields: Vec<SlangField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlangField {
    pub name: CompactString,
    pub offset: SlangUnit,
    pub layout: TypeLayout,
    pub ty: SlangType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlangArray {
    pub layout: TypeLayout,
    pub element_count: u32,
    pub element_type: Box<SlangType>,
}

/// 64-bit GPU device address for BDA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SlangAddress;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlangResource {
    Sampler,
    Texture(SlangTexture),
    Buffer(SlangBuffer),
}

impl From<&SlangResource> for vk::DescriptorType {
    fn from(resource: &SlangResource) -> Self {
        match resource {
            SlangResource::Sampler => vk::DescriptorType::SAMPLER,
            SlangResource::Texture(tex) => tex.into(),
            SlangResource::Buffer(buf) => buf.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlangTexture {
    pub dim: TextureDim,
    pub array: bool,
    pub multisampled: bool,
    pub access: TextureAccess,
    pub sampled: SampledType,
}

impl From<&SlangTexture> for vk::DescriptorType {
    fn from(tex: &SlangTexture) -> Self {
        match tex.access {
            TextureAccess::ReadOnly => vk::DescriptorType::SAMPLED_IMAGE,
            TextureAccess::ReadWrite => vk::DescriptorType::STORAGE_IMAGE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextureDim {
    One,
    Two,
    Three,
    Cube,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextureAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SampledType {
    Scalar(ScalarKind),
    Vector { scalar: ScalarKind, count: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlangBuffer {
    pub kind: BufferKind,
    pub access: BufferAccess,
    pub element: BufferElement,
    pub block_alignment_bytes: u32,
    pub trailing_array: Option<TrailingArray>,
}

impl From<&SlangBuffer> for vk::DescriptorType {
    fn from(buf: &SlangBuffer) -> Self {
        match (buf.kind, buf.access) {
            (BufferKind::Uniform, _) => vk::DescriptorType::UNIFORM_BUFFER,
            (BufferKind::Storage, BufferAccess::ReadOnly) => vk::DescriptorType::STORAGE_BUFFER,
            (BufferKind::Storage, BufferAccess::ReadWrite) => vk::DescriptorType::STORAGE_BUFFER,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BufferKind {
    Uniform,
    Storage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BufferAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BufferElement {
    RawBytes,
    Typed(SlangType),
}

/// A trailing unsized array in an SSBO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailingArray {
    pub element_layout: TypeLayout,
    pub element_type: Box<SlangType>,
}
