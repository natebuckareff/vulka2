## shader compilation and linking

this file is the spec for the `slang` crate in this repo

the `slang` crate in this repo is a wrapper around [slang-rs](https://github.com/FloatyMonkey/slang-rs). it's responsible for:
- compiling and linking slang shaders
- providing reflection information to the renderer for pipeline creation

```rust
struct SlangCompilerBuilder {
    fn new() -> Self;

    // finds the capability in the global session and sets it on the compiler
    // settings, otherwise errors
    fn capability(mut self, name: &str) -> Result<Self>;

    fn bindless_space_index(mut self, index: i32) -> Self;
    fn optimization(mut self, ...) -> Self;
    fn matrix_layout_row(mut self, ...) -> Self;

    // add directory where cached program layouts and spirv code are written
    fn cache_path<P: AsRef<Path>>(mut self, P) -> Self;

    // adds a search path for resolving modules
    fn search_path<P: AsRef<Path>>(mut self, P) -> Self;

    // instantiates a compiler instance
    fn build(self) -> SlangCompiler;
}
```

hardcode vulkan_use_entry_point_name(true)
hardcode format spirv
hardcode profile spirv_1_6 (this is guaranteed by min vk 1.3)

NOTE: we use term "entrypoint", while slang-rs uses "entry_point"

all of these `SlangCompilerBuilder` option values need to be incorporated into calculated `SlangProgramKey`s

```rust
// sort components by modules first, then entrypoints
// sort modules by some module-unique identifier
// sort entrypoints by name

struct SlangCompiler {
    // stages the module for compilation and returns a SlangModule for entrypoint selection
    fn load_module(&mut self, path) -> Result<SlangModule>;

    // returns a linker builder
    fn linker(&mut self) -> SlangLinker;
}

// link components
// pull in the modules for each entrypoint so progression looks like:
// [] start with empty component set
// [moduleA, epA_V, epA_F] add two entrypoints, pull in module
// [moduleA, moduleB, epA_V, epA_F, epB_F] add one entrypoint, pull in module
// sort components by modules first, then entrypoints
// sort modules by some module-unique identifier (module id)
// sort entrypoints by (stage, name, module id)
//
// components are sorted into a canonical ordering for deterministic layouts and
// stable cache key hashing
struct SlangLinker {
    // adds the module *and* all of its entrypoints
    fn add_module(mut self, module: &SlangModule) -> Self;

    // adds all entrypoints for a specific stage from a module. also adds the
    // module to the component list if it wasn't already
    fn add_stage(mut self, module: &SlangModule, stage) -> Self;

    // adds all entrypoints, regardless of stage, in a module. also adds the
    // module to the component list if it wasn't already
    fn add_all_entrypoints(mut self, module: &SlangModule) -> Self;

    // adds a single entrypoint. also adds the parent module to the component
    // list if it wasn't already
    fn add_entrypoint(mut self, entrypoint: SlangEntrypoint) -> Self;

    // defer calling any slang compilation functions _until link() is called_ so
    // that we can compute the `SlangProgramKey` for the current compiler+linker
    // state and potentially reuse a previously compiled artifact
    fn link(self) -> Result<SlangProgram>;
}
```

given the same set of modules + entrypoints, the `slang` crate defines a canonical composition ordering. layout/binding ordering follows this policy, not caller insertion order

```rust
struct SlangModule {
    fn module_id(&self) -> ModuleId;
    fn entrypoints(&self) -> impl Iterator<Item = SlangEntrypoint>;
    fn entrypoint(&self, stage, name: &str) -> Result<SlangEntrypoint>;
}

// bump this whenever we need to release a new version and invalidate old cache
// entries
const SLANG_CACHE_KEY_VERSION: u8 = 1;

// copyable handle to the entrypoint
struct SlangEntrypoint {
    name: CompactString, // own the name for simplicity

    fn module_id(&self) -> ModuleId;
    fn stage(&self) -> SlangShaderStage;
    fn name(&self) -> &str
}
```

`SlangEntrypoint` needs to be an unambiguous handle to the underlying `shader-slang` entrypoint owned by the module / compiler. Ideally we have some internal ID that maps back, or some Arc to the parent object

```rust
enum SlangShaderStage {
    Vertex,
    Fragment,
    Compute,
}

impl Into<vk::ShaderStageFlags> for SlangShaderStage { ... }

// can assume there is only ever one target so always layout(0)
// implies always entry_point_code(e, 0) for some entrypoint
struct SlangProgram {
    // the entire program and all of its compiled spirv blobs are cached
    fn cache_key(&self) -> SlangProgramKey;

    // get the program layout
    fn layout(&self) -> SlangLayout;

    // get the entrypoints in this program
    fn entrypoints(&self) -> impl Iterator<Item = SlangEntrypoint>;

    // get the program bytecode
    fn code(&self, entrypoint: SlangEntrypoint) -> SpirvCode

    // errors if any one stage has multiple entrypoints, but allows missing
    // stages. this is the easy path for most simple shaders
    fn select_all(&self) -> Result<SlangPipelineProgram>;

    // selects only graphics entrypoints, errors if any one selected stage has
    // multiple entrypoints, allows missing stages. selects at most one
    // graphics, and one fragment entrypoint, ignores compute
    fn select_graphics(&self) -> Result<SlangPipelineProgram>;

    // errors if multiple compute entrypoints, otherwise selects the single
    // compute entrypoint
    fn select_compute(&self) -> Result<SlangPipelineProgram>;

    // another helper: useful for large shader and we only need to grab a single
    // compute entrypoint
    fn select_one(
        &self,
        entrypoint: SlangEntrypoint
    ) -> Result<SlangPipelineProgram>;

    fn select_each(
        &self,
        selection: SlangPipelineSelection
    ) -> Result<SlangPipelineProgram>;
}

struct SlangPipelineSelection {
    vertex: Option<SlangEntrypoint>,
    fragment: Option<SlangEntrypoint>,
    compute: Option<SlangEntrypoint>,
}

// otherwise is the same as SlangProgram, just with a
// single-entrypoint-per-stage guarantee for vulkan pipeline creation
// should probably hold an Arc<SlangProgram> internally
struct SlangPipelineProgram {
    fn layout(&self) -> &SlangLayout;
    fn entrypoint(&self, stage: SlangShaderStage) -> Option<SlangEntrypoint>;
    fn entrypoints(&self) -> impl Iterator<Item = SlangEntrypoint>;
    fn code(&self, stage: SlangShaderStage) -> Option<&SpirvCode>;
}
```

the `SlangCompiler` must maintain an internal "program cache" so that when the same compiler options, modules, and endpoints are requested, it can compute the `SlangProgramKey` and look up an existing `SlangProgram` in the program cache

that cache should internally hold items like this (example):
```rust
struct SlangProgramCache {
    layouts: HashMap<SlangProgramKey, SlangLayout>,
    blobs: HashMap<SpirvCodeKey, SpirvCode>,
}
```

the rationale for not have a `HashMap<SlangProgramKey, SlangProgram>` is because: the cache check happens during `SlangLinker::link()` so at that point we know all the entrypoints requested and have constructed a `SlangProgramKey`, so deriving a `SlangProgram` is therefore trivial. The `SlangProgram` is implied by what we are trying to link! A cache hit skips recomputing the layout and spirv code, but will know the entrypoints from the linker state

also node that both `SlangLayout` and `SpirvCode` can be serialized/deserialized to disk using serde. `SpirvCode` will be byte serialized and `SlangLayout` will be serialized to JSON

```rust
impl Clone        for SpirvCode { ... }
impl AsRef<[u32]> for SpirvCode { ... } // dereference to 32-bit words

// just an owned blob of spirv bytes
// guaranteed to be 4-byte aligned
#[derive(Serialize, Deserialize)]
impl SpirvCode {
    fn cache_key() -> SpirvCodeKey;
    fn len_words() -> usize;
    fn len_bytes() -> usize;
}

// hash key calculated from:
// - slang compiler version
// - global version number
// - compiler options (the non-hardcoded ones)
// - all linked components (their sorted order confers stable identity)
//   - modules -> module source blake3 hash
//   - entrypoints -> parent module hash + entrypoint name + stage
impl Clone     for SlangProgramKey { ... }
impl Copy      for SlangProgramKey { ... }
impl PartialEq for SlangProgramKey { ... }
impl Eq        for SlangProgramKey { ... }
impl Hash      for SlangProgramKey { ... }
```

when calculating the hash of a module, need to use reflection to iterate over all the files that were included as dependencies, and also calculate their hashes

```rust
// hash key calculated from:
// - the parent program's cache key
// - the entrypoint stage + name
impl Clone     for SpirvCodeKey { ... }
impl Copy      for SpirvCodeKey { ... }
impl PartialEq for SpirvCodeKey { ... }
impl Eq        for SpirvCodeKey { ... }

fn example() {
    let mut compiler = SlangCompilerBuilder::new()
        .capability("SPV_EXT_physical_storage_buffer")?
        .capability("SPV_EXT_descriptor_indexing")?
        .optimization(OptimizationLevel::High)
        .matrix_layout_row(true)
        .search_path("./shaders")
        .build()?;

    let module1 = compiler.load_module("./shaders/shader1.slang")?;
    let module2 = compiler.load_module("./shaders/shader2.slang")?;
    let module3 = compiler.load_module("./shaders/lib.slang")?;

    let program = compiler.linker()
        .add_all_entrypoints(&module1)
        .add_entrypoint(module2.entrypoint("main")?)
        .add_module(module3)
        .link()?;

    // slang compiler is not called internally until the `link()` call. until
    // that point, we're just building up the cache key for the current program,
    // and then checking if a cached version already exists or not. if one is
    // already cached, we can just return that instead

    program.layout(); // -> used to generate pipelines
    program.code();   // -> returns 4-byte aligned spir-v code

    // example of how this would be used from an engine with vulkan builder
    // functions
    let pipeline = {
        let mut shader_modules = vec![];
        for entrypoint in program.entrypoints() {
            let code = program.code(entrypoint);
            // -> VkShaderModule
            shader_modules = build_vulkan_shader_module(code)?;
        }
        // -> VkPipelineLayout + set layouts + push constants etc
        build_vulkan_pipeline(program.layout(), shader_modules)?
    };
}
```

## shader layout

it's important to state up-front: most of the design decisions made in this section and the [shader types](#shader-types) section are meant to simplify the process of code generating "shader cursors" for writing from the CPU into these GPU memory resources

see this article [A practical and scalable approach to cross-platform shader parameter passing using Slang’s reflection API][1] for more information about the shader cursor concept

```rust
trait ByteLike {
    fn size_bytes(&self) -> u32;
}

trait ArrayLike {
    fn stride_bytes(&self) -> u32;
}

#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SlangUnit {
    set_spaces: u32,
    binding_slots: u32,
    bytes: u32,
}

// same semantics as in slang's shader cursor docs
impl Add for SlangUnit { ... }
impl Mul<u32> for SlangUnit { ... }
```

in the context of the slang reflection api:
- `set_spaces` maps to the `SubElementRegisterSpace` category
- `binding_slots` maps to the `DescriptorTableSlot` category
- `bytes` maps to the `Uniform` category

the idea is that `SlangUnit` values are _relative_ and _accumulated_ while walking the slang reflection API output tree. see [1] for details

```rust
#[derive(Serialize, Deserialize)]
struct SlangLayout {
    pub bindless_heap: Option<BindlessHeapLayout>,
    pub push_constants: Option<PushConstantLayout>,
    pub descriptor_sets: Vec<DescriptorSetLayout>,
    pub parameter_blocks: Vec<ParameterBlockLayout>,
    pub entrypoints: Vec<EntrypointLayout>,
}
```

`descriptor_sets` is the flattened Vulkan view used to build pipeline objects; `parameter_blocks` preserves the hierarchical shader-object structure used for cursor traversal and CPU writes

note that _all `SlangLayout` structs and types need to be `serde` serializable and deserializble_ so that we can cache to disk

```rust
struct BindlessHeapLayout {
    pub set: u32,
    pub policy: BindlessPolicy, // TODO VkMutable vs None ???
    pub bindings: Vec<DescriptorBindingLayout>,
}

enum BindlessPolicy {
    // TODO
}
```

still need to decide which bindless implementation we want to map to on the vulkan side. options are:
- descriptor indexing on a regular, runtime-sized descriptor set
- VK_EXT_descriptor_buffer
- something else?

can decide on this later, it doesn't really block finishing the `slang` crate

bindless heap extraction is unimplemented initially; `bindless_heap` will be `None` until supported


```rust
// impl ByteLike
struct PushConstantLayout {
    pub stages: vk::ShaderStageFlags,
    pub size_bytes: u32,
    pub ty: SlangStruct,
}

struct DescriptorSetLayout {
    pub set: u32,
    pub bindings: Vec<DescriptorBindingLayout>,
}

struct DescriptorBindingLayout {
    pub binding: u32,
    pub name: CompactString,
    pub flags: vk::DescriptorBindingFlags,
    pub stages: vk::ShaderStageFlags,
    pub ty: SlangResource,
    pub count: DescriptorCount,
}

enum DescriptorCount {
    One,
    Many(u32),
    Runtime,
}
```

```rust
// set and binding *must* be Vulkan descriptor set/binding indices, not Slang
// internal indices
struct ParameterBlockLayout {
    scope: ParameterBlockScope,
    name: CompactString,
    ty: SlangType,
    set: u32,
    ordinary: Option<OrdinaryParameterBinding>,
    bindings: Vec<DescriptorBindingLayout>,
    nested: Vec<ParameterBlockLayout>,
}

enum ParameterBlockScope {
    Global,
    Entrypoint(SlangEntrypoint),
}

struct OrdinaryParameterBinding {
    binding: u32,
    ty: SlangStruct,
}

struct EntrypointLayout {
    pub entrypoint: SlangEntrypoint,
    pub vertex_inputs: Option<VertexInputLayout>,
}

struct VertexInputLayout {
    pub attributes: Vec<VertexAttributeLayout>,
}

struct VertexAttributeLayout {
    pub location: u32,
    pub name: CompactString,
    pub format: vk::Format,
    pub hint_binding: u32, // hints are intended as reasonable defaults
    pub hint_offset: u32,  // ...
}
```

`hint_binding` and `hint_offset` are the "default recommended values", and are what the struct generation macros will use. the engine can use their own values instead, but will need to build their own vertex structs in that case

## shader types

```rust
// impl ByteLike
enum SlangType {
    Scalar(SlangScalar),
    Vector(SlangVector),
    Matrix(SlangMatrix),
    Struct(SlangStruct),
    Array(Box<SlangArray>),
    DescriptorHandle(Box<SlangResource>),
    DeviceAddress,
}

// impl ByteLike
enum SlangScalar {
    Bool,
    Int32,
    UInt32,
    Float32,
}

// impl ByteLike
enum SlangVector {
    Vec2(SlangScalar),
    Vec3(SlangScalar),
    Vec4(SlangScalar),
}

// impl ByteLike
enum SlangMatrix {
    Mat2x2(SlangScalar),
    Mat2x3(SlangScalar),
    Mat2x4(SlangScalar),
    Mat3x2(SlangScalar),
    Mat3x3(SlangScalar),
    Mat3x4(SlangScalar),
    Mat4x2(SlangScalar),
    Mat4x3(SlangScalar),
    Mat4x4(SlangScalar),
}

// impl ByteLike
struct SlangStruct {
    pub size: SlangUnit,
    pub alignment_bytes: u32,
    pub fields: Vec<SlangField>,
}

// impl ByteLike
struct SlangField {
    pub name: CompactString,
    pub offset: SlangUnit,
    pub size: SlangUnit,
    pub alignment_bytes: u32,
    pub ty: SlangType,
}

// impl ByteLike
// impl ArrayLike
struct SlangArray {
    pub stride: SlangUnit,
    pub alignment_bytes: u32,
    pub element_count: u32,
    pub ty: SlangType,
}

// impl ByteLike
struct DeviceAddress {
    ...
}

enum SlangResource {
    Sampler,
    Texture(SlangTexture),
    Buffer(SlangBuffer),
}

impl Into<vk::DescriptorType> for SlangResource { ... }

struct SlangTexture {
    pub dim: TextureDim,
    pub array: bool,
    pub multisampled: bool,
    pub access: TextureAccess,
    pub sampled: SlangSampledType,
}

enum TextureDim {
    One,
    Two,
    Three,
    Cube,
}

enum TextureAccess {
    ReadOnly,
    ReadWrite,
}

enum SlangSampledType {
    Scalar(SlangScalar),
    Vector(SlangVector),
}

struct SlangBuffer {
    pub kind: BufferKind,
    pub access: BufferAccess,
    pub element: BufferElement,
    pub block_alignment_bytes: u32
    pub trailing_array: Option<TrailingArray>,
}

enum BufferKind {
    Uniform,
    Storage,
}

enum BufferAccess {
    ReadOnly,
    ReadWrite,
}

// impl ByteLike
enum BufferElement {
    RawBytes,
    Typed(SlangType),
}

// impl ArrayLike
struct TrailingArray {
    pub stride: SlangUnit,
    pub alignment_bytes: u32,
    pub ty: SlangType,
}
```

## notes on the slang reflection API

- need to translate from slang offsets to [_actual_ vulkan descriptor indices](#shader-slang-issue-7598)
- be careful choosing between `getElementTypeLayout()` and `getElementVarLayout()` when getting the correct offsets within `ConstantBuffer<T>`s and `ParameterBlock<T>` (see `docs/slang-docs-md/08-compiling.md#reflection` locally or https://shader-slang.org/slang/user-guide/reflection). always prefer var-layout when extracting offsets for container members

## struct generation macros / shader cursor generation

TODO

## hot reload

TODO pre-requisites:
- implement `SlangProgram` caching
- implement `SlangLayout` and `SpirvCode` serdes
- will need engine hooks; dirty pipelines need to be re-created

## shader-slang issue 7598

from: https://github.com/shader-slang/slang/issues/7598

> When using the Slang Reflection API to extract descriptor set and binding information for Vulkan, it’s not immediately clear that the setIndex returned from getBindingRangeDescriptorSetIndex() is not the actual Vulkan descriptor set index, but rather the internal Slang set index. Similarly, the rangeIndex used with getDescriptorSetDescriptorRangeType() is not the actual Vulkan binding index.

> To retrieve the actual Vulkan descriptor set index, you must use getDescriptorSetSpaceOffset(), passing in the Slang descriptor set index obtained from getBindingRangeDescriptorSetIndex(). Similarly, to get the Vulkan binding index, you need to call getDescriptorSetDescriptorRangeIndexOffset(), passing both the Slang descriptor set index and the descriptor range index obtained from getBindingRangeFirstDescriptorRangeIndex().

## references

[1]: ../vendor/shader-slang-docs/shader-cursors.md
