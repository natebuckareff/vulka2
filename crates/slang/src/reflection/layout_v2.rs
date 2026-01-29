use compact_str::CompactString;
use vulkanalia::vk;

use crate::SlangShaderStage;

struct ShaderLayout {
    push_constants: Vec<PushConstantLayout>,
    descriptor_sets: Vec<DescriptorSet>,
    globals: Vec<VarLayout>,
    entrypoints: Vec<EntrypointLayout>,
}

// TODO: think about aliasing
struct PushConstantLayout {
    name: CompactString,
    offset_bytes: usize,
    size_bytes: usize,
    stages: vk::ShaderStageFlags,
    layout: StructLayout,
}

struct EntrypointLayout {
    stage: SlangShaderStage,
    params: Vec<VarLayout>,
}

// ------------------------------

struct VarLayout {
    name: CompactString,
    size: Size,
    value: ValueLayout,
}

enum ValueLayout {
    Pod(PodLayout),
    Struct(StructLayout),
    Array(ArrayLayout),
    Resource(ResourceLayout),
    ParameterBlock(ParameterBlockLayout),
    ConstantBuffer(ConstantBufferLayout),
}

// ------------------------------

struct PodLayout {
    offset: PodOffset,
    ty: PodType,
}

// NOTE: pod data does not need a "layout" since layout is a property of the
// container and pod data is always "inside" some kind of slang resource
// container
enum PodType {
    Scalar(ScalarType),
    Vector(VectorType),
    Matrix(MatrixType),
}

struct ScalarType {
    ty: CompactString,
}

struct VectorType {
    element: ScalarType,
    count: u32,
}

struct MatrixType {
    element: ScalarType,
    rows: u32,
    cols: u32,
}

struct StructLayout {
    offset: AggregateOffset,
    fields: Vec<VarLayout>,
}

struct ArrayLayout {
    offset: AggregateOffset,
    element: Box<ValueLayout>,
    count: ElementCount,
    stride: Stride,
}

struct ResourceLayout {
    offset: DescriptorOffset,
    ty: CompactString,
    kind: ResourceKind,
    element: Box<ValueLayout>,
    count: ElementCount,
    stride: Stride,
}

// slang resource types
// NOTE: ParameterBlock and ConstantBuffer are excluded from this type since we
// treat them specially in the layout tree. They're not "normal" resources but
// more so "layout nodes"
enum ResourceKind {
    // TODO
}

// NOTE: Binding 0 is the PB's uniform buffer binding iff the PB contains any
// ordinary (uniform) data that needs wrapping. If there are no ordinary bytes,
// then there is no implicit UBO, and the first resource will be at binding 0.
struct ParameterBlockLayout {
    descriptor_set_index: usize, // unique for all `ParameterBlockLayout`s
    element: Box<ValueLayout>,
}

// resets byte offsets in the element layout
struct ConstantBufferLayout {
    offset: DescriptorOffset,
    element: Box<ValueLayout>,
}

// ------------------------------

struct Size(usize); // bytes *excluding* padding

struct Stride(usize); // bytes *including* padding

enum ElementCount {
    Bounded(u32),
    Runtime,
}

enum AggregateOffset {
    Pod(PodOffset),
    Descriptor(DescriptorOffset),
}

struct PodOffset {
    offset_bytes: usize,
}

// NOTE: these are relative units, not addressible vulkan indices
struct DescriptorOffset {
    binding_index: u32, // indexes into current DescriptorSet::bindings
    array_index: u32,   // indexes into current descriptor array
}

// ------------------------------

// NOTE: set is None until at least one non-empty descriptor is added
struct DescriptorSet {
    set: Option<u32>,
    bindings: Vec<DescriptorBinding>,
}

// NOTE: binding is None util the descriptor is known to not be empty. Empty
// descriptors are filtered at the end of the relfection pass
struct DescriptorBinding {
    binding: Option<u32>,
    stages: vk::ShaderStageFlags, // OR of all stages that use this descriptor
    count: ElementCount,
    descriptor: Descriptor,
}

enum Descriptor {
    Pod(PodDescriptor),
    Opaque(DescriptorType), // non byte-addressible resources
}

// uniforms, ssbos, etc
struct PodDescriptor {
    size_bytes: usize,
    alignment_bytes: usize,
    ty: DescriptorType,
    // TODO: buffer class?
}

// maps to vulkan descriptor types
enum DescriptorType {
    // TODO
}
