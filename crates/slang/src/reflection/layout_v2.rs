use anyhow::Result;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use shader_slang as slang;
use vulkanalia::vk;

use crate::SlangShaderStage;
use crate::reflection::serde_slang::serde_binding_type;
use crate::reflection::serde_slang::serde_resource_access;
use crate::reflection::serde_slang::serde_resource_shape;
use crate::reflection::serde_vk::serde_descriptor_type;
use crate::reflection::serde_vk::serde_shader_stage_flags;

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct ShaderLayout {
    pub push_constants: Vec<PushConstantLayout>,
    pub descriptor_sets: Vec<DescriptorSet>,
    pub globals: Vec<VarLayout>,
    pub entrypoints: Vec<EntrypointLayout>,
}

// TODO: think about aliasing
#[derive(Clone, Serialize, Deserialize)]
pub struct PushConstantLayout {
    pub name: CompactString,
    pub offset_bytes: usize,
    pub size_bytes: usize,
    #[serde(with = "serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags,
    element: ValueLayout,
}

impl PushConstantLayout {
    pub fn new(
        name: CompactString,
        offset_bytes: usize,
        size_bytes: usize,
        stages: vk::ShaderStageFlags,
        element: ValueLayout,
    ) -> Result<Self> {
        if !element.is_pod() {
            return Err(anyhow::anyhow!("PushConstantLayout element must be pod"));
        }
        Ok(Self {
            name,
            offset_bytes,
            size_bytes,
            stages,
            element,
        })
    }

    pub fn element(&self) -> &ValueLayout {
        &self.element
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EntrypointLayout {
    pub stage: SlangShaderStage,
    pub params: Vec<VarLayout>,
}

// ------------------------------

trait Layout {
    fn is_pod(&self) -> bool;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VarLayout {
    pub name: CompactString,
    pub size: Size,
    pub value: ValueLayout,
}

impl Layout for VarLayout {
    fn is_pod(&self) -> bool {
        self.value.is_pod()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ValueLayout {
    Numeric(NumericLayout),
    Struct(StructLayout),
    Array(ArrayLayout),
    Resource(ResourceLayout),
    ParameterBlock(ParameterBlockLayout),
    ConstantBuffer(ConstantBufferLayout),
}

impl Layout for ValueLayout {
    fn is_pod(&self) -> bool {
        use ValueLayout::*;
        match self {
            Numeric(layout) => layout.is_pod(),
            Struct(layout) => layout.is_pod(),
            Array(layout) => layout.is_pod(),
            Resource(_) => false,
            ParameterBlock(_) => false,
            ConstantBuffer(_) => true,
        }
    }
}

// ------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct NumericLayout {
    pub offset: PodOffset,
    pub ty: NumericType,
}

impl Layout for NumericLayout {
    fn is_pod(&self) -> bool {
        true
    }
}

// NOTE: numeric data does not need a "layout" since layout is a property of the
// container and pod data is always "inside" some kind of slang resource
// container
#[derive(Clone, Serialize, Deserialize)]
pub enum NumericType {
    Scalar(ScalarType),
    Vector(VectorType),
    Matrix(MatrixType),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ScalarType {
    pub ty: CompactString,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VectorType {
    pub element: ScalarType,
    pub count: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MatrixType {
    pub element: ScalarType,
    pub rows: u32,
    pub cols: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StructLayout {
    pub offset: AggregateOffset,
    pub fields: Vec<VarLayout>,
}

impl Layout for StructLayout {
    fn is_pod(&self) -> bool {
        self.fields.iter().all(|field| field.is_pod())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArrayLayout {
    pub offset: AggregateOffset,
    pub element: Box<ValueLayout>,
    pub count: ElementCount,
    pub stride: Stride,
}

impl Layout for ArrayLayout {
    fn is_pod(&self) -> bool {
        self.element.is_pod()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ResourceLayout {
    pub offset: DescriptorOffset,
    pub ty: CompactString,
    #[serde(with = "serde_binding_type")]
    pub kind: ResourceKind,
    pub element: Option<Box<ValueLayout>>,
    pub count: ElementCount,
    pub stride: Stride,
}

impl Layout for ResourceLayout {
    fn is_pod(&self) -> bool {
        false
    }
}

// slang resource types
// NOTE: ParameterBlock and ConstantBuffer are excluded from this type since we
// treat them specially in the layout tree. They're not "normal" resources but
// more so "layout nodes"
// TODO: refine resource kind mapping beyond Slang binding types.
pub type ResourceKind = slang::BindingType;

// NOTE: Binding 0 is the PB's uniform buffer binding iff the PB contains any
// ordinary (uniform) data that needs wrapping. If there are no ordinary bytes,
// then there is no implicit UBO, and the first resource will be at binding 0.
#[derive(Clone, Serialize, Deserialize)]
pub struct ParameterBlockLayout {
    pub descriptor_set_index: usize, // unique for all `ParameterBlockLayout`s
    pub element: Box<ValueLayout>,
}

// resets byte offsets in the element layout
#[derive(Clone, Serialize, Deserialize)]
pub struct ConstantBufferLayout {
    pub offset: DescriptorOffset,
    element: Box<ValueLayout>,
}

impl ConstantBufferLayout {
    pub fn new(offset: DescriptorOffset, element: ValueLayout) -> Result<Self> {
        if !element.is_pod() {
            return Err(anyhow::anyhow!("ConstantBufferLayout element must be pod"));
        }
        Ok(Self {
            offset,
            element: Box::new(element),
        })
    }

    pub fn element(&self) -> &ValueLayout {
        &self.element
    }
}

// ------------------------------

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Size(pub usize); // bytes *excluding* padding

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Stride(pub usize); // bytes *including* padding

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ElementCount {
    Bounded(u32),
    Runtime,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum AggregateOffset {
    Pod(PodOffset),
    Descriptor(DescriptorOffset),
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PodOffset {
    pub offset_bytes: usize,
}

// NOTE: these are relative units, not addressible vulkan indices
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct DescriptorOffset {
    pub binding_index: u32, // indexes into current DescriptorSet::bindings
    pub array_index: u32,   // indexes into current descriptor array
}

// ------------------------------

// NOTE: set is None until at least one non-empty descriptor is added
#[derive(Clone, Serialize, Deserialize)]
pub struct DescriptorSet {
    pub set: Option<u32>,
    pub bindings: Vec<DescriptorBinding>,
}

// NOTE: binding is None util the descriptor is known to not be empty. Empty
// descriptors are filtered at the end of the relfection pass
#[derive(Clone, Serialize, Deserialize)]
pub struct DescriptorBinding {
    pub binding: Option<u32>,
    #[serde(with = "serde_shader_stage_flags")]
    pub stages: vk::ShaderStageFlags, // OR of all stages that use this descriptor
    pub count: ElementCount,
    pub descriptor: Descriptor,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Descriptor {
    #[serde(with = "serde_descriptor_type")]
    pub ty: vk::DescriptorType,
    #[serde(with = "serde_resource_shape")]
    pub shape: slang::ResourceShape,
    #[serde(with = "serde_resource_access")]
    pub access: slang::ResourceAccess,
    pub element: Option<Box<ValueLayout>>,
}
